#!/usr/bin/env python3
"""Migrate Neo4j chunk embeddings from base64 strings to LIST<FLOAT>.

Usage:
    python3 scripts/migrate_embeddings_to_list.py bolt://localhost:7687 neo4j password voidmdev
"""

import base64
import struct
import sys
from neo4j import GraphDatabase


def decode_embedding(base64_str: str) -> list[float]:
    """Decode a base64-encoded f32 array into a Python list of floats."""
    raw = base64.b64decode(base64_str)
    dim = len(raw) // 4
    floats = struct.unpack(f"<{dim}f", raw)
    return [float(v) for v in floats]


def migrate(tx, db_name: str):
    result = tx.run("""
        MATCH (c:MemoryChunk)
        WHERE c.embedding IS NOT NULL
        RETURN c.id AS id, c.embedding AS embedding, c.embedding_dim AS dim
    """)

    migrated = 0
    skipped = 0

    for record in result:
        chunk_id = record["id"]
        embedding = record["embedding"]
        dim = record["dim"]

        # If already a list, skip
        if isinstance(embedding, list):
            skipped += 1
            continue

        if isinstance(embedding, str):
            try:
                vec = decode_embedding(embedding)
                if len(vec) != dim:
                    print(f"  WARN dimension mismatch for {chunk_id}: expected {dim}, got {len(vec)}")
                    continue

                tx.run("""
                    MATCH (c:MemoryChunk {id: $id})
                    SET c.embedding = $embedding
                """, id=chunk_id, embedding=vec)
                migrated += 1
                if migrated % 100 == 0:
                    print(f"  Migrated {migrated} chunks...")
            except Exception as e:
                print(f"  FAIL {chunk_id}: {e}")

    return migrated, skipped


def main():
    if len(sys.argv) < 5:
        print("Usage: migrate_embeddings_to_list.py <uri> <user> <password> <db>")
        sys.exit(1)

    uri = sys.argv[1]
    user = sys.argv[2]
    password = sys.argv[3]
    db = sys.argv[4]

    driver = GraphDatabase.driver(uri, auth=(user, password))

    with driver.session(database=db) as session:
        print(f"Starting migration on {db}...")
        migrated, skipped = session.execute_write(lambda tx: migrate(tx, db))
        print(f"Done. Migrated {migrated}, skipped {skipped} (already LIST<FLOAT>).")
        if migrated > 0:
            print("Creating vector index (if not exists)...")
            session.run("""
                CREATE VECTOR INDEX chunk_embedding IF NOT EXISTS
                FOR (c:MemoryChunk) ON c.embedding
                OPTIONS {indexConfig: {`vector.dimensions`: 384, `vector.similarity_function`: 'cosine'}}
            """)
            print("Index created.")

    driver.close()


if __name__ == "__main__":
    main()
