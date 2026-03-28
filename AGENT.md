# voidm
Help AI assistant to store and search memories using cli.
Memories are splitted into chunks before embeddings.
Memories are auto tagged on add.
Memories are auto linked on add.


# voidm-cli
Hardrules : backend agnostic. 
All DB operation must be done using DB trait
Cypher query is first class citizen.

# vodm-sqlite
sqlite backend, with cyper to sql.
support vector search with the help of [sqlite-vector](https://github.com/sqliteai/sqlite-vector)
cypher to sql mus be the prefered interface when interactig with the backend.