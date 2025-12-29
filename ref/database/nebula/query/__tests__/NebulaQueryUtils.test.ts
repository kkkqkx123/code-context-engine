import { NebulaQueryUtils, QueryType } from '../NebulaQueryUtils';

describe('NebulaQueryUtils', () => {
  const utils = new NebulaQueryUtils();

  describe('interpolateParameters - 参数插值', () => {
    it('应该在查询中插值参数', () => {
      const query = 'SELECT * FROM nodes WHERE id = :id AND name = :name';
      const parameters = { id: '123', name: 'test' };

      const result = NebulaQueryUtils.interpolateParameters(query, parameters);

      expect(result).toContain('123');
      expect(result).toContain('"test"');
    });

    it('当没有参数时应返回原始查询', () => {
      const query = 'SELECT * FROM nodes';
      const result = NebulaQueryUtils.interpolateParameters(query, {});

      expect(result).toBe(query);
    });

    it('应该处理多次出现的同一参数', () => {
      const query = 'SELECT * WHERE id = :id OR parentId = :id';
      const parameters = { id: '123' };

      const result = NebulaQueryUtils.interpolateParameters(query, parameters);

      const matches = result.match(/123/g);
      expect(matches?.length).toBe(2);
    });

    it('应该不处理undefined的参数', () => {
      const query = 'SELECT * FROM nodes';
      const result = NebulaQueryUtils.interpolateParameters(query, undefined as any);

      expect(result).toBe(query);
    });

    it('应该使用实例方法插值', () => {
      const query = 'SELECT * WHERE id = :id';
      const parameters = { id: '456' };

      const result = utils.interpolateParameters(query, parameters);

      expect(result).toContain('456');
    });
  });

  describe('escapeValue - 转义值', () => {
    it('应该转义字符串值', () => {
      const result = NebulaQueryUtils.escapeValue('test');
      expect(result).toBe('"test"');
    });

    it('应该转义包含引号的字符串', () => {
      const result = NebulaQueryUtils.escapeValue('test"quote');
      expect(result).toContain('\\"');
    });

    it('应该转义包含反斜杠的字符串', () => {
      const result = NebulaQueryUtils.escapeValue('test\\path');
      expect(result).toContain('\\\\');
    });

    it('应该处理数字值', () => {
      const result = NebulaQueryUtils.escapeValue(123);
      expect(result).toBe('123');
    });

    it('应该处理布尔值', () => {
      expect(NebulaQueryUtils.escapeValue(true)).toBe('true');
      expect(NebulaQueryUtils.escapeValue(false)).toBe('false');
    });

    it('应该将null转换为NULL', () => {
      const result = NebulaQueryUtils.escapeValue(null);
      expect(result).toBe('NULL');
    });

    it('应该将undefined转换为NULL', () => {
      const result = NebulaQueryUtils.escapeValue(undefined);
      expect(result).toBe('NULL');
    });

    it('应该处理数组值', () => {
      const result = NebulaQueryUtils.escapeValue([1, 2, 3]);
      expect(result).toContain('[');
      expect(result).toContain(']');
    });

    it('应该处理对象值', () => {
      const result = NebulaQueryUtils.escapeValue({ key: 'value' });
      expect(result).toBeDefined();
    });

    it('应该转义单引号', () => {
      const result = NebulaQueryUtils.escapeValue("it's");
      expect(result).toContain("\\'");
    });

    it('应该使用实例方法转义值', () => {
      const result = utils.escapeValue('test');
      expect(result).toBe('"test"');
    });
  });

  describe('escapeProperties - 转义属性', () => {
    it('应该转义对象中的所有属性', () => {
      const properties = { id: '123', name: 'test' };
      const result = NebulaQueryUtils.escapeProperties(properties);

      expect(result.id).toBe('123');
      expect(result.name).toBe('"test"');
    });

    it('应该处理混合类型的属性', () => {
      const properties = {
        id: 123,
        name: 'test',
        active: true,
        data: null
      };

      const result = NebulaQueryUtils.escapeProperties(properties);

      expect(result.id).toBe('123');
      expect(result.name).toContain('test');
      expect(result.active).toBe('true');
      expect(result.data).toBe('NULL');
    });

    it('应该使用实例方法转义属性', () => {
      const properties = { id: '123' };
      const result = utils.escapeProperties(properties);

      expect(result.id).toBe('123');
    });
  });

  describe('validateQuery - 查询验证', () => {
    it('应该验证合法的查询', () => {
      const result = NebulaQueryUtils.validateQuery('SELECT * FROM nodes');
      expect(result).toBe(true);
    });

    it('应该拒绝空查询', () => {
      const result = NebulaQueryUtils.validateQuery('');
      expect(result).toBe(false);
    });

    it('应该拒绝仅包含空格的查询', () => {
      const result = NebulaQueryUtils.validateQuery('   ');
      expect(result).toBe(false);
    });

    it('应该拒绝DROP SPACE查询', () => {
      const result = NebulaQueryUtils.validateQuery('DROP SPACE test_space');
      expect(result).toBe(false);
    });

    it('应该拒绝DELETE FROM查询', () => {
      const result = NebulaQueryUtils.validateQuery('DELETE FROM nodes');
      expect(result).toBe(false);
    });

    it('应该拒绝TRUNCATE查询', () => {
      const result = NebulaQueryUtils.validateQuery('TRUNCATE nodes');
      expect(result).toBe(false);
    });

    it('应该拒绝包含SYSTEM的查询', () => {
      const result = NebulaQueryUtils.validateQuery('SYSTEM query');
      expect(result).toBe(false);
    });

    it('应该处理undefined的查询', () => {
      const result = NebulaQueryUtils.validateQuery(undefined as any);
      expect(result).toBe(false);
    });

    it('应该处理非字符串的查询', () => {
      const result = NebulaQueryUtils.validateQuery(123 as any);
      expect(result).toBe(false);
    });

    it('应该使用实例方法验证查询', () => {
      const result = utils.validateQuery('MATCH (v) RETURN v');
      expect(result).toBe(true);
    });
  });

  describe('detectQueryType - 检测查询类型', () => {
    it('应该检测DDL查询', () => {
      expect(NebulaQueryUtils.detectQueryType('CREATE SPACE test')).toBe(QueryType.DDL);
      expect(NebulaQueryUtils.detectQueryType('DROP SPACE test')).toBe(QueryType.DDL);
      expect(NebulaQueryUtils.detectQueryType('ALTER SPACE test')).toBe(QueryType.DDL);
    });

    it('应该检测DML查询', () => {
      expect(NebulaQueryUtils.detectQueryType('INSERT INTO nodes')).toBe(QueryType.DML);
      expect(NebulaQueryUtils.detectQueryType('UPDATE nodes')).toBe(QueryType.DML);
      expect(NebulaQueryUtils.detectQueryType('DELETE FROM nodes')).toBe(QueryType.DML);
    });

    it('应该检测QUERY查询', () => {
      expect(NebulaQueryUtils.detectQueryType('MATCH (v) RETURN v')).toBe(QueryType.QUERY);
      expect(NebulaQueryUtils.detectQueryType('FETCH PROP ON node1')).toBe(QueryType.QUERY);
      expect(NebulaQueryUtils.detectQueryType('GO FROM v')).toBe(QueryType.QUERY);
    });

    it('应该检测ADMIN查询', () => {
      expect(NebulaQueryUtils.detectQueryType('SHOW SPACES')).toBe(QueryType.ADMIN);
      expect(NebulaQueryUtils.detectQueryType('DESCRIBE SPACE test')).toBe(QueryType.ADMIN);
    });

    it('应该检测OTHER查询', () => {
      const result = NebulaQueryUtils.detectQueryType('UNKNOWN COMMAND');
      expect(result).toBe(QueryType.OTHER);
    });

    it('应该不区分大小写', () => {
      expect(NebulaQueryUtils.detectQueryType('match (v) return v')).toBe(QueryType.QUERY);
      expect(NebulaQueryUtils.detectQueryType('Create Space test')).toBe(QueryType.DDL);
    });

    it('应该处理前导空格', () => {
      const result = NebulaQueryUtils.detectQueryType('  SELECT * FROM nodes');
      expect(result).toBeDefined();
    });

    it('应该使用实例方法检测查询类型', () => {
      const result = utils.detectQueryType('INSERT INTO nodes');
      expect(result).toBe(QueryType.DML);
    });
  });

  describe('复杂场景', () => {
    it('应该处理包含特殊字符的查询', () => {
      const query = 'SELECT * FROM nodes WHERE name = :name';
      const parameters = { name: 'test\'s "node"' };

      const result = NebulaQueryUtils.interpolateParameters(query, parameters);

      expect(result).toBeDefined();
      expect(result).not.toContain(':name');
    });

    it('应该处理包含多种数据类型的参数', () => {
      const query = 'INSERT INTO nodes (id, score, active) VALUES (:id, :score, :active)';
      const parameters = {
        id: '123',
        score: 9.5,
        active: true
      };

      const result = NebulaQueryUtils.interpolateParameters(query, parameters);

      expect(result).toContain('123');
      expect(result).toContain('9.5');
      expect(result).toContain('true');
    });

    it('应该组合多个操作', () => {
      const properties = { name: 'test', age: 25 };
      const escaped = NebulaQueryUtils.escapeProperties(properties);
      const validated = NebulaQueryUtils.validateQuery('INSERT INTO nodes');

      expect(escaped).toBeDefined();
      expect(validated).toBe(true);
    });
  });
});
