import { LRUCache } from '../LRUCache';

describe('LRUCache', () => {
  let cache: LRUCache<string, number>;

  beforeEach(() => {
    cache = new LRUCache<string, number>(3); // 设置最大大小为3以便测试淘汰
  });

  describe('constructor', () => {
    it('应该使用默认最大大小初始化', () => {
      const defaultCache = new LRUCache<string, number>();
      expect(defaultCache.size()).toBe(0);
      // 默认大小应该是1000
      defaultCache.set('key1', 1);
      defaultCache.set('key2', 2);
      // 可以设置超过2个值，说明默认大小大于2
      expect(defaultCache.size()).toBe(2);
    });

    it('应该使用自定义最大大小初始化', () => {
      const customCache = new LRUCache<string, number>(2);
      expect(customCache.size()).toBe(0);

      customCache.set('key1', 1);
      customCache.set('key2', 2);
      expect(customCache.size()).toBe(2);
    });
  });

  describe('get', () => {
    it('应该返回缓存的值', () => {
      cache.set('key1', 100);
      expect(cache.get('key1')).toBe(100);
    });

    it('应该返回undefined如果键不存在', () => {
      expect(cache.get('nonexistent')).toBeUndefined();
    });

    it('应该在获取时更新访问时间（LRU行为）', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      // 访问key1使其成为最近使用的
      expect(cache.get('key1')).toBe(1);

      // 添加key4，应该淘汰key2（最久未使用的）
      cache.set('key4', 4);

      expect(cache.get('key1')).toBe(1); // key1应该还存在
      expect(cache.get('key2')).toBeUndefined(); // key2应该被淘汰
      expect(cache.get('key3')).toBe(3); // key3应该还存在
      expect(cache.get('key4')).toBe(4); // key4应该存在
    });
  });

  describe('set', () => {
    it('应该设置缓存值', () => {
      cache.set('key1', 100);
      expect(cache.get('key1')).toBe(100);
      expect(cache.size()).toBe(1);
    });

    it('应该更新已存在键的值', () => {
      cache.set('key1', 100);
      cache.set('key1', 200);
      expect(cache.get('key1')).toBe(200);
      expect(cache.size()).toBe(1);
    });

    it('应该在达到最大大小时淘汰最久未使用的项', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      // 访问key1使其成为最近使用的
      cache.get('key1');

      // 添加key4，应该淘汰key2（最久未使用的）
      cache.set('key4', 4);

      expect(cache.size()).toBe(3);
      expect(cache.get('key1')).toBe(1); // key1应该还存在
      expect(cache.get('key2')).toBeUndefined(); // key2应该被淘汰
      expect(cache.get('key3')).toBe(3); // key3应该还存在
      expect(cache.get('key4')).toBe(4); // key4应该存在
    });

    it('应该处理零大小缓存', () => {
      const zeroCache = new LRUCache<string, number>(0);
      zeroCache.set('key1', 1);
      expect(zeroCache.size()).toBe(0);
      expect(zeroCache.get('key1')).toBeUndefined();
    });
  });

  describe('has', () => {
    it('应该返回true如果键存在', () => {
      cache.set('key1', 100);
      expect(cache.has('key1')).toBe(true);
    });

    it('应该返回false如果键不存在', () => {
      expect(cache.has('nonexistent')).toBe(false);
    });
  });

  describe('delete', () => {
    it('应该删除指定的键', () => {
      cache.set('key1', 100);
      expect(cache.has('key1')).toBe(true);

      const result = cache.delete('key1');
      expect(result).toBe(true);
      expect(cache.has('key1')).toBe(false);
      expect(cache.size()).toBe(0);
    });

    it('应该返回false如果键不存在', () => {
      const result = cache.delete('nonexistent');
      expect(result).toBe(false);
    });
  });

  describe('clear', () => {
    it('应该清空所有缓存', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      expect(cache.size()).toBe(2);

      cache.clear();
      expect(cache.size()).toBe(0);
      expect(cache.get('key1')).toBeUndefined();
      expect(cache.get('key2')).toBeUndefined();
    });
  });

  describe('size', () => {
    it('应该返回正确的缓存大小', () => {
      expect(cache.size()).toBe(0);

      cache.set('key1', 1);
      expect(cache.size()).toBe(1);

      cache.set('key2', 2);
      expect(cache.size()).toBe(2);

      cache.delete('key1');
      expect(cache.size()).toBe(1);

      cache.clear();
      expect(cache.size()).toBe(0);
    });
  });

  describe('keys', () => {
    it('应该返回所有键的数组', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      const keys = cache.keys();
      expect(keys).toContain('key1');
      expect(keys).toContain('key2');
      expect(keys).toContain('key3');
      expect(keys.length).toBe(3);
    });
  });

  describe('values', () => {
    it('应该返回所有值的数组', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      const values = cache.values();
      expect(values).toContain(1);
      expect(values).toContain(2);
      expect(values).toContain(3);
      expect(values.length).toBe(3);
    });
  });

  describe('LRU行为', () => {
    it('应该正确处理LRU淘汰顺序', () => {
      // 填满缓存
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      // 访问key1使其成为最近使用的
      cache.get('key1');

      // 访问key2使其成为第二最近使用的
      cache.get('key2');

      // 添加key4，应该淘汰key3（最久未使用的）
      cache.set('key4', 4);

      expect(cache.get('key1')).toBe(1);
      expect(cache.get('key2')).toBe(2);
      expect(cache.get('key3')).toBeUndefined();
      expect(cache.get('key4')).toBe(4);
    });

    it('应该在设置已存在的键时更新其LRU状态', () => {
      cache.set('key1', 1);
      cache.set('key2', 2);
      cache.set('key3', 3);

      // 更新key1的值，这应该使其成为最近使用的
      cache.set('key1', 100);

      // 添加key4，应该淘汰key2（现在是最久未使用的）
      cache.set('key4', 4);

      expect(cache.get('key1')).toBe(100);
      expect(cache.get('key2')).toBeUndefined();
      expect(cache.get('key3')).toBe(3);
      expect(cache.get('key4')).toBe(4);
    });
  });
});