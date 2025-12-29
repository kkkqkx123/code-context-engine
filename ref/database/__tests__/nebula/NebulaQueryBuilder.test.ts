import { NebulaQueryBuilder } from '../../nebula/query/NebulaQueryBuilder';

describe('NebulaQueryBuilder', () => {
  let queryBuilder: NebulaQueryBuilder;

  beforeEach(() => {
    queryBuilder = new NebulaQueryBuilder();
  });

  describe('insertVertex', () => {
    it('should build INSERT VERTEX query without properties', () => {
      const result = queryBuilder.insertVertex('TestTag', 'vertex123', {});

      expect(result.query).toBe('INSERT VERTEX TestTag() VALUES "vertex123":()');
      expect(result.params).toEqual({});
    });

    it('should build INSERT VERTEX query with properties', () => {
      const result = queryBuilder.insertVertex('TestTag', 'vertex123', {
        name: 'Test Node',
        age: 25,
        active: true
      });

      expect(result.query).toBe('INSERT VERTEX TestTag(name, age, active) VALUES "vertex123":($param0, $param1, $param2)');
      expect(result.params).toEqual({
        param0: 'Test Node',
        param1: 25,
        param2: true
      });
    });

    it('should handle properties with special characters', () => {
      const result = queryBuilder.insertVertex('TestTag', 'vertex123', {
        'name-with-dash': 'Test',
        'name_with_underscore': 'Test',
        'name with space': 'Test'
      });

      expect(result.query).toBe('INSERT VERTEX TestTag(name-with-dash, name_with_underscore, name with space) VALUES "vertex123":($param0, $param1, $param2)');
      expect(result.params).toEqual({
        param0: 'Test',
        param1: 'Test',
        param2: 'Test'
      });
    });
  });

  describe('batchInsertVertices', () => {
    it('should return empty query for empty vertices array', () => {
      const result = queryBuilder.batchInsertVertices([]);

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });

    it('should build batch INSERT VERTEX query for same tag vertices', () => {
      const vertices = [
        { id: 'vertex1', tag: 'TestTag', properties: { name: 'Node1', age: 20 } },
        { id: 'vertex2', tag: 'TestTag', properties: { name: 'Node2', age: 30 } },
      ];

      const result = queryBuilder.batchInsertVertices(vertices);

      expect(result.query).toBe(
        'INSERT VERTEX TestTag(name, age) VALUES "vertex1":($TestTag_vertex1_param0, $TestTag_vertex1_param1), "vertex2":($TestTag_vertex2_param0, $TestTag_vertex2_param1)'
      );
      expect(result.params).toEqual({
        'TestTag_vertex1_param0': 'Node1',
        'TestTag_vertex1_param1': 20,
        'TestTag_vertex2_param0': 'Node2',
        'TestTag_vertex2_param1': 30
      });
    });

    it('should build batch INSERT VERTEX query for different tag vertices', () => {
      const vertices = [
        { id: 'vertex1', tag: 'Tag1', properties: { name: 'Node1' } },
        { id: 'vertex2', tag: 'Tag2', properties: { name: 'Node2' } },
      ];

      const result = queryBuilder.batchInsertVertices(vertices);

      expect(result.query).toBe(
        'INSERT VERTEX Tag1(name) VALUES "vertex1":($Tag1_vertex1_param0); INSERT VERTEX Tag2(name) VALUES "vertex2":($Tag2_vertex2_param0)'
      );
      expect(result.params).toEqual({
        'Tag1_vertex1_param0': 'Node1',
        'Tag2_vertex2_param0': 'Node2'
      });
    });

    it('should handle vertices with different property sets', () => {
      const vertices = [
        { id: 'vertex1', tag: 'TestTag', properties: { name: 'Node1', age: 20 } },
        { id: 'vertex2', tag: 'TestTag', properties: { name: 'Node2', active: true } },
      ];

      const result = queryBuilder.batchInsertVertices(vertices);

      expect(result.query).toBe(
        'INSERT VERTEX TestTag(name, age, active) VALUES "vertex1":($TestTag_vertex1_param0, $TestTag_vertex1_param1, $TestTag_vertex1_param2), "vertex2":($TestTag_vertex2_param0, $TestTag_vertex2_param1, $TestTag_vertex2_param2)'
      );
      expect(result.params).toEqual({
        'TestTag_vertex1_param0': 'Node1',
        'TestTag_vertex1_param1': 20,
        'TestTag_vertex1_param2': null, // Missing active property for vertex1
        'TestTag_vertex2_param0': 'Node2',
        'TestTag_vertex2_param1': null, // Missing age property for vertex2
        'TestTag_vertex2_param2': true
      });
    });
  });

  describe('insertEdge', () => {
    it('should build INSERT EDGE query without properties', () => {
      const result = queryBuilder.insertEdge('TestEdge', 'src123', 'dst456', {});

      expect(result.query).toBe('INSERT EDGE TestEdge() VALUES "src123"->"dst456":()');
      expect(result.params).toEqual({});
    });

    it('should build INSERT EDGE query with properties', () => {
      const result = queryBuilder.insertEdge('TestEdge', 'src123', 'dst456', {
        weight: 1.0,
        type: 'friend',
        since: 2022
      });

      expect(result.query).toBe('INSERT EDGE TestEdge(weight, type, since) VALUES "src123"->"dst456":($param0, $param1, $param2)');
      expect(result.params).toEqual({
        param0: 1.0,
        param1: 'friend',
        param2: 2022
      });
    });
  });

  describe('batchInsertEdges', () => {
    it('should return empty query for empty edges array', () => {
      const result = queryBuilder.batchInsertEdges([]);

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });

    it('should build batch INSERT EDGE query for same type edges', () => {
      const edges = [
        { srcId: 'src1', dstId: 'dst1', type: 'TestEdge', properties: { weight: 1.0 } },
        { srcId: 'src2', dstId: 'dst2', type: 'TestEdge', properties: { weight: 2.0 } },
      ];

      const result = queryBuilder.batchInsertEdges(edges);

      expect(result.query).toBe(
        'INSERT EDGE TestEdge(weight) VALUES "src1"->"dst1":($TestEdge_src1_dst1_param0), "src2"->"dst2":($TestEdge_src2_dst2_param0)'
      );
      expect(result.params).toEqual({
        'TestEdge_src1_dst1_param0': 1.0,
        'TestEdge_src2_dst2_param0': 2.0
      });
    });

    it('should build batch INSERT EDGE query for different type edges', () => {
      const edges = [
        { srcId: 'src1', dstId: 'dst1', type: 'EdgeType1', properties: { weight: 1.0 } },
        { srcId: 'src2', dstId: 'dst2', type: 'EdgeType2', properties: { weight: 2.0 } },
      ];

      const result = queryBuilder.batchInsertEdges(edges);

      expect(result.query).toBe(
        'INSERT EDGE EdgeType1(weight) VALUES "src1"->"dst1":($EdgeType1_src1_dst1_param0); INSERT EDGE EdgeType2(weight) VALUES "src2"->"dst2":($EdgeType2_src2_dst2_param0)'
      );
      expect(result.params).toEqual({
        'EdgeType1_src1_dst1_param0': 1.0,
        'EdgeType2_src2_dst2_param0': 2.0
      });
    });

    it('should handle edges with different property sets', () => {
      const edges = [
        { srcId: 'src1', dstId: 'dst1', type: 'TestEdge', properties: { weight: 1.0, type: 'friend' } },
        { srcId: 'src2', dstId: 'dst2', type: 'TestEdge', properties: { weight: 2.0, active: true } },
      ];

      const result = queryBuilder.batchInsertEdges(edges);

      expect(result.query).toBe(
        'INSERT EDGE TestEdge(weight, type, active) VALUES "src1"->"dst1":($TestEdge_src1_dst1_param0, $TestEdge_src1_dst1_param1, $TestEdge_src1_dst1_param2), "src2"->"dst2":($TestEdge_src2_dst2_param0, $TestEdge_src2_dst2_param1, $TestEdge_src2_dst2_param2)'
      );
      expect(result.params).toEqual({
        'TestEdge_src1_dst1_param0': 1.0,
        'TestEdge_src1_dst1_param1': 'friend',
        'TestEdge_src1_dst1_param2': null, // Missing active property for first edge
        'TestEdge_src2_dst2_param0': 2.0,
        'TestEdge_src2_dst2_param1': null, // Missing type property for second edge
        'TestEdge_src2_dst2_param2': true
      });
    });
  });

  describe('go', () => {
    it('should build GO query without edge type', () => {
      const result = queryBuilder.go(2, 'vertex123', 'dst(edge) AS destination');

      expect(result).toBe('GO 2 STEPS FROM "vertex123" YIELD dst(edge) AS destination');
    });

    it('should build GO query with edge type', () => {
      const result = queryBuilder.go(3, 'vertex123', 'dst(edge) AS destination', 'FRIEND');

      expect(result).toBe('GO 3 STEPS FROM "vertex123" OVER FRIEND YIELD dst(edge) AS destination');
    });

    it('should handle different yield fields', () => {
      const result = queryBuilder.go(1, 'vertex456', 'id(vertex) AS vid, properties(vertex) AS props');

      expect(result).toBe('GO 1 STEPS FROM "vertex456" YIELD id(vertex) AS vid, properties(vertex) AS props');
    });
  });

  describe('buildComplexTraversal', () => {
    it('should build complex traversal query with default options', () => {
      const result = queryBuilder.buildComplexTraversal('start123', ['EDGE_TYPE']);

      expect(result.query).toContain('GO 3 STEPS FROM $startId OVER EDGE_TYPE');
      expect(result.params).toEqual({ startId: 'start123' });
    });

    it('should build complex traversal query with custom options', () => {
      const options = {
        maxDepth: 5,
        filterConditions: ['n.age > 18', 'n.active == true'],
        returnFields: ['n.name', 'n.age'],
        limit: 20
      };

      const result = queryBuilder.buildComplexTraversal('start123', ['EDGE_TYPE', 'ANOTHER_EDGE'], options);

      expect(result.query).toContain('GO 5 STEPS FROM $startId OVER EDGE_TYPE,ANOTHER_EDGE');
      expect(result.query).toContain('WHERE n.age > 18 AND n.active == true');
      expect(result.query).toContain('YIELD n.name, n.age');
      expect(result.query).toContain('LIMIT 20');
      expect(result.params).toEqual({ startId: 'start123' });
    });

    it('should handle empty edge types', () => {
      const result = queryBuilder.buildComplexTraversal('start123', [], { maxDepth: 2 });

      expect(result.query).toContain('GO 2 STEPS FROM $startId OVER *');
      expect(result.params).toEqual({ startId: 'start123' });
    });
  });

  describe('buildShortestPath', () => {
    it('should build shortest path query with default parameters', () => {
      const result = queryBuilder.buildShortestPath('source123', 'target456');

      expect(result.query).toBe(
        '\n      FIND SHORTEST PATH FROM $sourceId TO $targetId OVER * UPTO 10 STEPS\n    '
      );
      expect(result.params).toEqual({
        sourceId: 'source123',
        targetId: 'target456'
      });
    });

    it('should build shortest path query with custom edge types', () => {
      const result = queryBuilder.buildShortestPath('source123', 'target456', ['EDGE1', 'EDGE2'], 5);

      expect(result.query).toBe(
        '\n      FIND SHORTEST PATH FROM $sourceId TO $targetId OVER EDGE1,EDGE2 UPTO 5 STEPS\n    '
      );
      expect(result.params).toEqual({
        sourceId: 'source123',
        targetId: 'target456'
      });
    });
  });

  describe('updateVertex', () => {
    it('should build UPDATE VERTEX query with properties', () => {
      const result = queryBuilder.updateVertex('vertex123', 'TestTag', {
        name: 'Updated Name',
        age: 30
      });

      expect(result.query).toContain('UPDATE VERTEX ON TestTag "vertex123"');
      expect(result.query).toContain('SET name = $set_param0, age = $set_param1');
      expect(result.params).toEqual({
        'set_param0': 'Updated Name',
        'set_param1': 30
      });
    });

    it('should return empty query for empty properties', () => {
      const result = queryBuilder.updateVertex('vertex123', 'TestTag', {});

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });
  });

  describe('updateEdge', () => {
    it('should build UPDATE EDGE query with properties', () => {
      const result = queryBuilder.updateEdge('src123', 'dst456', 'TestEdge', {
        weight: 2.5,
        type: 'updated'
      });

      expect(result.query).toContain('UPDATE EDGE ON TestEdge "src123" -> "dst456"');
      expect(result.query).toContain('SET weight = $set_param0, type = $set_param1');
      expect(result.params).toEqual({
        'set_param0': 2.5,
        'set_param1': 'updated'
      });
    });

    it('should return empty query for empty properties', () => {
      const result = queryBuilder.updateEdge('src123', 'dst456', 'TestEdge', {});

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });
  });

  describe('deleteVertices', () => {
    it('should build DELETE VERTEX query without tag', () => {
      const result = queryBuilder.deleteVertices(['vertex1', 'vertex2']);

      expect(result.query).toBe('DELETE VERTEX $id0, $id1');
      expect(result.params).toEqual({
        id0: 'vertex1',
        id1: 'vertex2'
      });
    });

    it('should build DELETE VERTEX query with tag', () => {
      const result = queryBuilder.deleteVertices(['vertex1', 'vertex2'], 'TestTag');

      expect(result.query).toBe('DELETE VERTEX $id0, $id1 TAG TestTag');
      expect(result.params).toEqual({
        id0: 'vertex1',
        id1: 'vertex2'
      });
    });

    it('should return empty query for empty vertex IDs array', () => {
      const result = queryBuilder.deleteVertices([]);

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });
  });

  describe('deleteEdges', () => {
    it('should build DELETE EDGE query', () => {
      const edges = [
        { srcId: 'src1', dstId: 'dst1', edgeType: 'TestEdge' },
        { srcId: 'src2', dstId: 'dst2', edgeType: 'TestEdge' },
      ];

      const result = queryBuilder.deleteEdges(edges);

      expect(result.query).toBe('DELETE EDGE $src0 -> $dst0, $src1 -> $dst1');
      expect(result.params).toEqual({
        src0: 'src1',
        dst0: 'dst1',
        src1: 'src2',
        dst1: 'dst2'
      });
    });

    it('should return empty query for empty edges array', () => {
      const result = queryBuilder.deleteEdges([]);

      expect(result.query).toBe('');
      expect(result.params).toEqual({});
    });
  });

  describe('buildNodeCountQuery', () => {
    it('should build node count query', () => {
      const result = queryBuilder.buildNodeCountQuery('TestTag');

      expect(result.query).toContain('MATCH (n:TestTag)');
      expect(result.query).toContain('RETURN count(n) AS total');
      expect(result.params).toEqual({});
    });
  });

  describe('buildRelationshipCountQuery', () => {
    it('should build relationship count query', () => {
      const result = queryBuilder.buildRelationshipCountQuery('TestEdge');

      expect(result.query).toContain('MATCH ()-[r:TestEdge]->()');
      expect(result.query).toContain('RETURN count(r) AS total');
      expect(result.params).toEqual({});
    });
  });
});