# semantic-facts-kotlin

Parser-derived semantic facts for Kotlin in Basalt.

## Capabilities

- `SEMANTIC_FACTS` / `semantic-facts@kotlin/v1`.
- Requires `parse.call-sites@kt/v1` and `parse.retrieval@kt/v1`
  (provided by `parser-kotlin`).

## Facts emitted

- `DeclareSymbol` for classes, objects, functions, and properties, qualified
  by package and enclosing types
  (`com.adevcorn.addhelper::AddHelperViewModel::refresh`).
- `Extends` for class delegation clauses (`: ViewModel(), ItemRepository`),
  ignoring primary-constructor parameter types.
- `AsyncBoundary` (`Await` for `suspend fun`, `Spawn` for
  `launch`/`async`/`withContext`, `Await` for `await`/`join`).
- `ReferenceSymbol` for `@Composable` annotations.
- `Calls` plus `IOOperation` classification for OkHttp/HTTP, file streams,
  and `println`/`Log` output.
- `ImportModule` and `DependsOn` for Kotlin imports, including aliases.

## Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

Artifact:
`target/wasm32-unknown-unknown/release/basalt_plugin_semantic_facts_kotlin.wasm`.
