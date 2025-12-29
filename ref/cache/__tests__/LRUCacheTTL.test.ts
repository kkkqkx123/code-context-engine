import { LRUCache } from '../LRUCache';

describe('LRUCache TTL Test', () => {
  test('should handle TTL correctly in LRUCache', async () => {
    const cache = new LRUCache<string, any>(10, {
      enableStats: true,
      defaultTTL: 300000 // 5 minutes default
    });

    const key = 'ttl-test';
    const value = { data: 'test-value' };

    console.log('Setting value with 50ms TTL...');
    cache.set(key, value, 50);

    // Check the entry directly
    const entry = (cache as any).cache.get(key);
    console.log('Entry after set:', {
      timestamp: entry?.timestamp,
      ttl: entry?.ttl,
      now: Date.now(),
      timeDiff: Date.now() - entry?.timestamp,
      willExpire: entry?.ttl ? (Date.now() - entry?.timestamp > entry?.ttl) : 'no ttl'
    });

    // Should be available immediately
    const immediateResult = cache.get(key);
    console.log('Immediate result:', immediateResult);
    expect(immediateResult).toEqual(value);

    // Wait for expiration
    console.log('Waiting for expiration...');
    await new Promise(resolve => setTimeout(resolve, 100));

    // Check the entry again
    const expiredEntry = (cache as any).cache.get(key);
    console.log('Entry after wait:', {
      timestamp: expiredEntry?.timestamp,
      ttl: expiredEntry?.ttl,
      now: Date.now(),
      timeDiff: Date.now() - expiredEntry?.timestamp,
      willExpire: expiredEntry?.ttl ? (Date.now() - expiredEntry?.timestamp > expiredEntry?.ttl) : 'no ttl'
    });

    // Should be expired now
    const expiredResult = cache.get(key);
    console.log('Expired result:', expiredResult);
    expect(expiredResult).toBeUndefined();

    // Check stats
    const stats = cache.getStats();
    console.log('Final stats:', stats);
  });

  test('should handle TTL with different values', async () => {
    const cache = new LRUCache<string, any>(10, {
      enableStats: true
    });

    // Test with different TTL values
    cache.set('short', { data: 'short' }, 50);
    cache.set('long', { data: 'long' }, 5000);
    cache.set('default', { data: 'default' }); // Should use defaultTTL if set

    // All should be available immediately
    expect(cache.get('short')).toEqual({ data: 'short' });
    expect(cache.get('long')).toEqual({ data: 'long' });
    expect(cache.get('default')).toEqual({ data: 'default' });

    // Wait for short TTL to expire
    await new Promise(resolve => setTimeout(resolve, 100));

    // Short should be expired, long and default should still be available
    expect(cache.get('short')).toBeUndefined();
    expect(cache.get('long')).toEqual({ data: 'long' });
    expect(cache.get('default')).toEqual({ data: 'default' });
  });
});