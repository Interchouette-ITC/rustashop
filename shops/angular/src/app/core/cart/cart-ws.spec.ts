import { describe, expect, it } from 'vitest';

import { cartWsUrl, parseCartUpdatedMessage } from './cart-ws';

describe('parseCartUpdatedMessage', () => {
  it('returns cart snapshot for cart.updated', () => {
    const raw = JSON.stringify({
      type: 'cart.updated',
      version: 1,
      cart: {
        id: 'cart-1',
        token: 'tok',
        status: 'open',
        currency: 'EUR',
        lines: [],
        items_total: { amount_minor: 0, currency: 'EUR' },
      },
    });
    const cart = parseCartUpdatedMessage(raw);
    expect(cart?.id).toBe('cart-1');
    expect(cart?.token).toBe('tok');
  });

  it('ignores unknown event types', () => {
    expect(parseCartUpdatedMessage(JSON.stringify({ type: 'job.log' }))).toBeNull();
  });

  it('ignores invalid JSON', () => {
    expect(parseCartUpdatedMessage('not-json')).toBeNull();
  });
});

describe('cartWsUrl', () => {
  it('maps absolute http base to ws', () => {
    expect(cartWsUrl('http://127.0.0.1:8080', 'c1', 't1')).toBe(
      'ws://127.0.0.1:8080/v1/carts/c1/ws?token=t1',
    );
  });

  it('maps relative /api base via location.origin', () => {
    const url = cartWsUrl('/api', 'c2', 'tok space');
    expect(url.startsWith('ws://') || url.startsWith('wss://')).toBe(true);
    expect(url).toContain('/api/v1/carts/c2/ws?token=tok%20space');
  });
});
