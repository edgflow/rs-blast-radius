# blast-radius

Predict what a code change might break. Before you change a function,
blast-radius parses your Rust codebase, builds its call graph, finds everything
that reaches the function, and asks an AI model to explain what your change
could break: which callers, tests and API consumers, and why.

Instead of asking AI to write code, ask it to predict the consequences of
changing code.

```text
$ blast-radius impact examples/shop apply_discount
pricing::apply_discount  (src/pricing.rs:4)
Risk: HIGH (78/100)
  - 2 function(s) call it directly
  - 6 function(s) depend on it, across 4 module(s)
  - 2 HTTP endpoint(s) reach it: their clients are outside this codebase
  - it is public: code outside this repository may call it too
  - 3 test(s) reach it

What reaches it (9):
  1  certain   cart::Cart::total → pricing::apply_discount
  2  likely    checkout::quote → cart::Cart::total → pricing::apply_discount
  3  likely    api::get_quote → checkout::quote → cart::Cart::total → pricing::apply_discount
  …
```

## Use it

```bash
cargo build --release
cargo run -- functions examples/shop          # every function: kind, calls, location
cargo run -- graph examples/shop              # the call graph and its most depended-on functions
cargo run -- impact examples/shop apply_discount
cp .env.example .env                          # then set OPENAI_API_KEY
cargo run -- predict examples/shop pricing::apply_discount \
  --change "Return Result<u64, PricingError> and reject percentages above 100" \
  --out report.md
```

`examples/shop` is a small shop backend to try it on: pricing, cart, checkout,
inventory, HTTP handlers and tests. Point it at any Rust codebase; it only needs
to parse, not build.

## How it works

1. **Parse** (`src/parse.rs`): `syn` reads every `.rs` file and records each
   function (qualified name, kind, visibility, location) and every call it makes,
   including calls inside macros like `assert_eq!`. Functions marked `#[test]` are
   tests; handlers marked `#[get("/…")]`, `#[post(…)]`, … are HTTP endpoints.
2. **Graph** (`src/graph.rs`): calls are resolved through modules and `use`
   items. A path that resolves exactly is *certain*; a method call is matched by
   name (*likely* when one type has that method, *possible* when several do),
   because there are no types without compiling.
3. **Blast radius** (`src/impact.rs`): walk the callers backwards from the
   function. The risk score adds up direct callers, dependents, HTTP endpoints,
   public visibility and missing tests, each with a reason you can check.
4. **Predict** (`src/predict.rs`): the model gets the change, the function's
   source and only the functions in its blast radius. Its answer is strict JSON,
   then checked against the graph: anything it names that the change doesn't
   reach is removed and listed. The final risk is the higher of the graph's and
   the model's.

## Limits

Static analysis without types is an estimate. Trait methods, function pointers
and macros that generate calls are approximated or missed, which is why each
edge carries a confidence. Code sent to the model leaves your machine: only the
changed function and its blast radius are sent, and responses are not stored.

## Test

```bash
cargo test     # no network: the OpenAI API is replaced by a local stand-in
```
