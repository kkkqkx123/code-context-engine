import { HashUtils } from '../HashUtils';

describe('HashUtils', () => {
  describe('simpleHash', () => {
    it('should generate consistent hashes for the same input', () => {
      const input = 'test string';
      const hash1 = HashUtils.simpleHash(input);
      const hash2 = HashUtils.simpleHash(input);
      expect(hash1).toBe(hash2);
    });

    it('should generate different hashes for different inputs', () => {
      const hash1 = HashUtils.simpleHash('input1');
      const hash2 = HashUtils.simpleHash('input2');
      expect(hash1).not.toBe(hash2);
    });

    it('should handle empty string', () => {
      const hash = HashUtils.simpleHash('');
      expect(typeof hash).toBe('string');
      expect(hash).toBe('0');
    });

    it('should handle special characters', () => {
      const hash = HashUtils.simpleHash('hello world!@#$%^&*()');
      expect(typeof hash).toBe('string');
      expect(hash.length).toBeGreaterThan(0);
    });
  });

  describe('fnv1aHash', () => {
    it('should generate consistent hashes for the same input', () => {
      const input = 'test string';
      const hash1 = HashUtils.fnv1aHash(input);
      const hash2 = HashUtils.fnv1aHash(input);
      expect(hash1).toBe(hash2);
    });

    it('should generate different hashes for different inputs', () => {
      const hash1 = HashUtils.fnv1aHash('input1');
      const hash2 = HashUtils.fnv1aHash('input2');
      expect(hash1).not.toBe(hash2);
    });

    it('should handle empty string', () => {
      const hash = HashUtils.fnv1aHash('');
      expect(typeof hash).toBe('string');
      expect(hash).toBe('0');
    });

    it('should handle special characters', () => {
      const hash = HashUtils.fnv1aHash('hello world!@#$%^&*()');
      expect(typeof hash).toBe('string');
      expect(hash.length).toBeGreaterThan(0);
    });
  });

  describe('calculateStringHash', () => {
    it('should generate consistent hashes for the same input', () => {
      const input = 'test string';
      const hash1 = HashUtils.calculateStringHash(input);
      const hash2 = HashUtils.calculateStringHash(input);
      expect(hash1).toBe(hash2);
    });

    it('should generate different hashes for different inputs', () => {
      const hash1 = HashUtils.calculateStringHash('input1');
      const hash2 = HashUtils.calculateStringHash('input2');
      expect(hash1).not.toBe(hash2);
    });
  });

  describe('generateId', () => {
    it('should generate different IDs for each call', () => {
      const id1 = HashUtils.generateId();
      const id2 = HashUtils.generateId();
      expect(id1).not.toBe(id2);
    });

    it('should include the prefix', () => {
      const id = HashUtils.generateId('test');
      expect(id).toMatch(/^test_/);
    });
  });

  describe('Performance', () => {
    it('should handle large strings efficiently', () => {
      const largeString = 'a'.repeat(10000);
      const start = performance.now();
      const hash = HashUtils.simpleHash(largeString);
      const end = performance.now();

      expect(typeof hash).toBe('string');
      expect(end - start).toBeLessThan(100); // Should complete in under 100ms
    });
  });
});