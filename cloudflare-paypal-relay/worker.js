// PayPal tip relay for the twitch-bot-rs PayPal watcher (see ../src/paypal.rs).
//
// The bot runs on a home PC with no public address, so PayPal can't call it
// directly. This Worker is the public endpoint PayPal calls instead:
//
//   1. PayPal POSTs a webhook to /paypal-webhook whenever a payment lands
//      in the connected PayPal Business account (paypal.me links included).
//   2. This Worker verifies the webhook's signature with PayPal's own
//      verification API (never trust an unverified webhook body — anyone
//      could POST a fake "$500 tip" here otherwise).
//   3. Verified tips are stored in KV, one key per tip.
//   4. The bot polls GET /pending-tips on an interval, which drains
//      (reads + deletes) every stored tip and returns them as JSON.
//
// Required bindings (set in the Cloudflare dashboard, or wrangler.toml):
//   - KV namespace bound as TIPS_KV
//   - Secrets: PAYPAL_CLIENT_ID, PAYPAL_CLIENT_SECRET, PAYPAL_WEBHOOK_ID,
//     RELAY_TOKEN (a long random string you also put in the bot's .env as
//     PAYPAL_RELAY_TOKEN — this is what stops /pending-tips being scraped
//     by anyone who finds the Worker's URL).
//   - Optional: PAYPAL_API_BASE — defaults to the Live API. Set it to
//     https://api-m.sandbox.paypal.com (along with Sandbox values for the
//     four secrets above) to test against PayPal's free Sandbox instead of
//     real money — a genuine Sandbox checkout produces a properly-signed
//     webhook, unlike the Webhooks Simulator's unsigned "mock" events.
//
// lokati.net/tip (tip.html in the site folder) is a second, direct source:
//   - POST /api/tip/order creates a PayPal order for a server-validated
//     amount and parks the tipper's name/message in KV for an hour.
//   - POST /api/tip/capture {orderID} captures it server-side and queues the
//     tip only when PayPal says COMPLETED; amount/currency come from PayPal's
//     capture response, never from the page.
// Both this path and the webhook key the tip by PayPal's capture ID
// (tip:<capture_id>) and set seen:<capture_id>, so the webhook that PayPal
// also fires for a /tip payment is dropped as a duplicate instead of
// alerting twice. Routed as lokati.net/api/tip* (same pattern as
// lokati-feed-cache's lokati.net/api/feed*).

const TIP_TTL_SECONDS = 60 * 60 * 24; // pending tips expire after a day, in case the bot's offline for a while
const SEEN_TTL_SECONDS = 60 * 60 * 24 * 7; // capture IDs already queued — outlasts PayPal's webhook retries
const ORDER_TTL_SECONDS = 60 * 60; // name/message parked between order creation and capture

export const TIP_MIN_USD = 1;
export const TIP_MAX_USD = 500;
const TIP_CURRENCY = 'USD';
const NAME_MAX_LEN = 50;
const MESSAGE_MAX_LEN = 255;

// The page may end up on a different origin than the Worker (if the relay
// can't be routed under lokati.net), so the /api/tip routes answer CORS for
// the site's own origin only.
const TIP_ALLOWED_ORIGIN = 'https://lokati.net';

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === 'POST' && url.pathname === '/paypal-webhook') {
      return handleWebhook(request, env);
    }

    if (request.method === 'GET' && url.pathname === '/pending-tips') {
      return handlePendingTips(request, env);
    }

    if (url.pathname === '/api/tip/order' || url.pathname === '/api/tip/capture') {
      if (request.method === 'OPTIONS') return withCors(new Response(null, { status: 204 }));
      if (request.method !== 'POST') return withCors(new Response('Method not allowed', { status: 405 }));
      const handler = url.pathname === '/api/tip/order' ? handleTipOrder : handleTipCapture;
      return withCors(await handler(request, env));
    }

    return new Response('Not found', { status: 404 });
  },
};

function apiBase(env) {
  return env.PAYPAL_API_BASE || 'https://api-m.paypal.com';
}

async function getPaypalAccessToken(env) {
  const creds = btoa(`${env.PAYPAL_CLIENT_ID}:${env.PAYPAL_CLIENT_SECRET}`);
  const resp = await fetch(`${apiBase(env)}/v1/oauth2/token`, {
    method: 'POST',
    headers: {
      Authorization: `Basic ${creds}`,
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: 'grant_type=client_credentials',
  });
  if (!resp.ok) throw new Error(`PayPal OAuth token request failed: ${resp.status}`);
  const data = await resp.json();
  return data.access_token;
}

async function isWebhookVerified(request, rawBody, env) {
  const accessToken = await getPaypalAccessToken(env);

  // .trim() guards against a stray trailing newline/space in the secret
  // value, a common paste artifact that would otherwise silently mismatch
  // the real webhook ID and fail every verification.
  const webhookId = (env.PAYPAL_WEBHOOK_ID || '').trim();

  const verifyResp = await fetch(`${apiBase(env)}/v1/notifications/verify-webhook-signature`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${accessToken}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      auth_algo: request.headers.get('paypal-auth-algo'),
      cert_url: request.headers.get('paypal-cert-url'),
      transmission_id: request.headers.get('paypal-transmission-id'),
      transmission_sig: request.headers.get('paypal-transmission-sig'),
      transmission_time: request.headers.get('paypal-transmission-time'),
      webhook_id: webhookId,
      webhook_event: JSON.parse(rawBody),
    }),
  });

  const data = await verifyResp.json().catch(() => null);

  if (!verifyResp.ok) {
    console.error('PayPal verify-webhook-signature HTTP error', verifyResp.status, JSON.stringify(data));
    return false;
  }

  if (!data || data.verification_status !== 'SUCCESS') {
    console.error('PayPal webhook signature not verified', JSON.stringify(data), 'webhookId used:', webhookId);
    return false;
  }

  return true;
}

// Pulls a payer display name and amount out of a PayPal webhook event.
// PayPal represents a received payment differently depending on how it
// arrived — a Checkout/Orders-API payment fires PAYMENT.CAPTURE.COMPLETED,
// while a plain P2P transfer (paypal.me, "Send money") fires
// PAYMENT.SALE.COMPLETED with an older resource shape — so both are
// handled here. If the real payload has payer info in a spot this doesn't
// expect, it'll fall back to "Anonymous" rather than fail the whole
// webhook; check the Worker's logs (wrangler tail) after a real test
// payment to see the exact shape and adjust the field paths below if
// needed.
function extractTip(event) {
  const resource = event.resource || {};
  const amount = resource.amount || {};

  const payerName =
    resource.payer_name ||
    (resource.payer && resource.payer.name &&
      `${resource.payer.name.given_name || ''} ${resource.payer.name.surname || ''}`.trim()) ||
    (resource.payer_info && resource.payer_info.first_name &&
      `${resource.payer_info.first_name} ${resource.payer_info.last_name || ''}`.trim()) ||
    null;

  return {
    id: resource.id || null,
    name: payerName || 'Anonymous',
    amount: parseFloat(amount.value || amount.total) || 0,
    currency: amount.currency_code || amount.currency || '',
    message: resource.note_to_payer || resource.note || '',
  };
}

const HANDLED_EVENT_TYPES = new Set(['PAYMENT.CAPTURE.COMPLETED', 'PAYMENT.SALE.COMPLETED']);

async function handleWebhook(request, env) {
  const rawBody = await request.text();

  let verified = false;
  try {
    verified = await isWebhookVerified(request, rawBody, env);
  } catch (err) {
    console.error('PayPal webhook verification error:', err);
    return new Response('Verification error', { status: 500 });
  }

  if (!verified) {
    return new Response('Invalid signature', { status: 400 });
  }

  const event = JSON.parse(rawBody);

  // PayPal fires webhooks for lots of event types (refunds, disputes,
  // subscription billing, etc.) — only completed incoming payments should
  // turn into a tip alert.
  if (!HANDLED_EVENT_TYPES.has(event.event_type)) {
    return new Response('Ignored', { status: 200 });
  }

  const tip = extractTip(event);

  // A /tip payment was already queued by the direct capture path — this is
  // PayPal's webhook for the same capture arriving as backup.
  if (tip.id && (await env.TIPS_KV.get(`seen:${tip.id}`))) {
    return new Response('Duplicate', { status: 200 });
  }

  await queueTip(env, tip);
  return new Response('OK', { status: 200 });
}

// Keyed by capture ID when there is one, so the direct path and the webhook
// land on the same key; seen: outlives the tip itself so a late webhook
// retry after the bot has drained the tip is still recognised.
async function queueTip(env, tip) {
  const key = tip.id ? `tip:${tip.id}` : `tip:${Date.now()}:${crypto.randomUUID()}`;
  await env.TIPS_KV.put(key, JSON.stringify(tip), { expirationTtl: TIP_TTL_SECONDS });
  if (tip.id) {
    await env.TIPS_KV.put(`seen:${tip.id}`, '1', { expirationTtl: SEEN_TTL_SECONDS });
  }
}

function withCors(response) {
  const headers = new Headers(response.headers);
  headers.set('Access-Control-Allow-Origin', TIP_ALLOWED_ORIGIN);
  headers.set('Access-Control-Allow-Methods', 'POST, OPTIONS');
  headers.set('Access-Control-Allow-Headers', 'Content-Type');
  headers.set('Vary', 'Origin');
  return new Response(response.body, { status: response.status, headers });
}

function jsonResponse(body, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

// Accepts a JSON number or a plain decimal string ("5", "12.50"), at most two
// decimal places, inside [TIP_MIN_USD, TIP_MAX_USD]. Returns PayPal's
// "value" string form, or null when the input is out of range or malformed.
export function validateTipAmount(raw) {
  const text = typeof raw === 'number' ? String(raw) : typeof raw === 'string' ? raw.trim() : '';
  if (!/^\d+(\.\d{1,2})?$/.test(text)) return null;
  const value = Number(text);
  if (!Number.isFinite(value) || value < TIP_MIN_USD || value > TIP_MAX_USD) return null;
  return value.toFixed(2);
}

function cleanText(raw, maxLen) {
  return typeof raw === 'string' ? raw.trim().slice(0, maxLen) : '';
}

async function handleTipOrder(request, env) {
  const body = await request.json().catch(() => null);
  if (!body || typeof body !== 'object') return jsonResponse({ error: 'Invalid request' }, 400);

  const value = validateTipAmount(body.amount);
  if (!value) {
    return jsonResponse({ error: `Amount must be between ${TIP_MIN_USD} and ${TIP_MAX_USD}` }, 400);
  }

  let order;
  try {
    const accessToken = await getPaypalAccessToken(env);
    const resp = await fetch(`${apiBase(env)}/v2/checkout/orders`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        intent: 'CAPTURE',
        purchase_units: [{ amount: { currency_code: TIP_CURRENCY, value }, description: 'Tip for Lokati' }],
      }),
    });
    order = await resp.json().catch(() => null);
    if (!resp.ok || !order || !order.id) {
      console.error('PayPal create order failed', resp.status, JSON.stringify(order));
      return jsonResponse({ error: 'Could not create order' }, 502);
    }
  } catch (err) {
    console.error('PayPal create order error:', err);
    return jsonResponse({ error: 'Could not create order' }, 502);
  }

  const meta = { name: cleanText(body.name, NAME_MAX_LEN), message: cleanText(body.message, MESSAGE_MAX_LEN) };
  await env.TIPS_KV.put(`order:${order.id}`, JSON.stringify(meta), { expirationTtl: ORDER_TTL_SECONDS });

  return jsonResponse({ id: order.id });
}

async function handleTipCapture(request, env) {
  const body = await request.json().catch(() => null);
  const orderID = body && typeof body.orderID === 'string' ? body.orderID.trim() : '';
  if (!/^[A-Za-z0-9-]{1,64}$/.test(orderID)) return jsonResponse({ error: 'Invalid order' }, 400);

  let result;
  try {
    const accessToken = await getPaypalAccessToken(env);
    const resp = await fetch(`${apiBase(env)}/v2/checkout/orders/${orderID}/capture`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${accessToken}`,
        'Content-Type': 'application/json',
      },
    });
    result = await resp.json().catch(() => null);
    if (!resp.ok || !result) {
      console.error('PayPal capture failed', resp.status, JSON.stringify(result));
      return jsonResponse({ error: 'Capture failed' }, 502);
    }
  } catch (err) {
    console.error('PayPal capture error:', err);
    return jsonResponse({ error: 'Capture failed' }, 502);
  }

  // Only a real COMPLETED capture becomes a tip; PENDING (e.g. an eCheck)
  // is left to the webhook, which fires when it actually completes.
  const unit = (result.purchase_units || [])[0] || {};
  const capture = ((unit.payments || {}).captures || [])[0];
  if (result.status !== 'COMPLETED' || !capture || capture.status !== 'COMPLETED' || !capture.id) {
    return jsonResponse({ status: (capture && capture.status) || result.status || 'UNKNOWN' }, 202);
  }

  const meta = JSON.parse((await env.TIPS_KV.get(`order:${orderID}`)) || '{}');
  const payerName =
    result.payer && result.payer.name &&
    `${result.payer.name.given_name || ''} ${result.payer.name.surname || ''}`.trim();
  const amount = capture.amount || {};

  await queueTip(env, {
    id: capture.id,
    name: meta.name || payerName || 'Anonymous',
    amount: parseFloat(amount.value) || 0,
    currency: amount.currency_code || '',
    message: meta.message || '',
  });
  await env.TIPS_KV.delete(`order:${orderID}`);

  return jsonResponse({ status: 'COMPLETED' });
}

async function handlePendingTips(request, env) {
  const auth = request.headers.get('Authorization') || '';
  if (auth !== `Bearer ${env.RELAY_TOKEN}`) {
    return new Response('Unauthorized', { status: 401 });
  }

  const list = await env.TIPS_KV.list({ prefix: 'tip:' });
  const tips = [];

  for (const key of list.keys) {
    const value = await env.TIPS_KV.get(key.name);
    if (value) tips.push(JSON.parse(value));
    await env.TIPS_KV.delete(key.name);
  }

  return new Response(JSON.stringify(tips), {
    headers: { 'Content-Type': 'application/json' },
  });
}
