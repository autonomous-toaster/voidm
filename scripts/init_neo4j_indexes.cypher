// Create fulltext index on Memory content (used for BM25 search)
CREATE FULLTEXT INDEX memories_content IF NOT EXISTS
FOR (m:Memory) ON EACH [m.content];

// Create vector index on MemoryChunk embedding (used for ANN search)
CREATE VECTOR INDEX chunk_embedding IF NOT EXISTS
FOR (c:MemoryChunk) ON c.embedding
OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}};
