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

const TIP_TTL_SECONDS = 60 * 60 * 24; // pending tips expire after a day, in case the bot's offline for a while

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === 'POST' && url.pathname === '/paypal-webhook') {
      return handleWebhook(request, env);
    }

    if (request.method === 'GET' && url.pathname === '/pending-tips') {
      return handlePendingTips(request, env);
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
  const key = `tip:${Date.now()}:${crypto.randomUUID()}`;
  await env.TIPS_KV.put(key, JSON.stringify(tip), { expirationTtl: TIP_TTL_SECONDS });

  return new Response('OK', { status: 200 });
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
