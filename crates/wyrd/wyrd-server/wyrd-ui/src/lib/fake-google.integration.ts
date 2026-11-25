/**
 * A loopback fake Google for the production-browser journeys. Test-only; it
 * runs inside the integration lanes.
 */
import { generateKeyPairSync, randomUUID, sign } from 'node:crypto';
import { createServer as createHttpServer } from 'node:http';

/** The Google account the fake Google signs in next. */
export type GoogleAccount = { sub: string; email: string; email_verified?: boolean; name?: string };

/**
 * A loopback stand-in for Google's OpenID provider, as Wyrd's controlled
 * test endpoints allow: `/authorize` signs `account` in at once and returns to
 * the caller's redirect URI, and `/token` answers an EdDSA-signed ID token
 * echoing that request's client and nonce. It listens on `port`, or on any
 * free port when `port` is 0. An account's email is verified unless it says
 * otherwise; its `name` claim is sent only when it has one.
 */
export async function startFakeGoogle(port = 0): Promise<{ issuer: string; account: GoogleAccount }> {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const pending = new Map<string, { aud: string; nonce: string; account: GoogleAccount }>();
  const google = { issuer: '', account: { sub: 'google-user', email: 'user@gmail.test' } };
  const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const server = createHttpServer((request, response) => {
    const url = new URL(request.url ?? '/', google.issuer);
    const json = (body: unknown) => {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JSON.stringify(body));
    };
    if (url.pathname === '/.well-known/openid-configuration') {
      return json({
        issuer: google.issuer,
        authorization_endpoint: `${google.issuer}/authorize`,
        token_endpoint: `${google.issuer}/token`,
        jwks_uri: `${google.issuer}/jwks`,
        response_types_supported: ['code'],
        subject_types_supported: ['public'],
        id_token_signing_alg_values_supported: ['EdDSA'],
        authorization_response_iss_parameter_supported: true
      });
    }
    if (url.pathname === '/jwks') return json({ keys: [{ ...publicKey.export({ format: 'jwk' }), kid: 'fake-google' }] });
    if (url.pathname === '/authorize') {
      const code = randomUUID();
      pending.set(code, { aud: url.searchParams.get('client_id') ?? '', nonce: url.searchParams.get('nonce') ?? '', account: { ...google.account } });
      const back = new URL(url.searchParams.get('redirect_uri') ?? '');
      back.searchParams.set('code', code);
      back.searchParams.set('state', url.searchParams.get('state') ?? '');
      back.searchParams.set('iss', google.issuer);
      response.writeHead(302, { location: back.toString() });
      return response.end();
    }
    if (url.pathname === '/token') {
      let form = '';
      request.on('data', (chunk: Buffer) => (form += chunk.toString()));
      request.on('end', () => {
        const grant = pending.get(new URLSearchParams(form).get('code') ?? '');
        if (!grant) {
          response.writeHead(400, { 'content-type': 'application/json' });
          return response.end(JSON.stringify({ error: 'invalid_grant' }));
        }
        const now = Math.floor(Date.now() / 1000);
        const signed = `${encode({ alg: 'EdDSA', typ: 'JWT', kid: 'fake-google' })}.${encode({
          iss: google.issuer, aud: grant.aud, sub: grant.account.sub, email: grant.account.email,
          email_verified: grant.account.email_verified ?? true, nonce: grant.nonce, iat: now, exp: now + 300, auth_time: now,
          ...(grant.account.name ? { name: grant.account.name } : {})
        })}`;
        json({
          access_token: randomUUID(), token_type: 'Bearer', expires_in: 300,
          id_token: `${signed}.${sign(null, Buffer.from(signed), privateKey).toString('base64url')}`
        });
      });
      return;
    }
    response.writeHead(404);
    response.end();
  });
  await new Promise<void>((resolve) => server.listen(port, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('the fake Google has no port');
  google.issuer = `http://127.0.0.1:${address.port}`;
  server.unref();
  return google;
}
