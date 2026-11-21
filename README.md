# Bill Echo

Bill Echo is a private, local-first web app that finds recurring charges and
price creep in bank-export CSV data. Paste an export into a browser page served
from your own machine; the Rust process groups noisy merchant descriptions,
recognizes common billing cadences, estimates yearly cost, and shows when each
charge may recur.

It has no database, telemetry, or outbound network requests.

## Run it

```console
git clone https://github.com/nicholas-ruest/bill-echo.git
cd bill-echo
cargo run --release
```

Open <http://127.0.0.1:3000>. To use another port:

```console
PORT=8080 cargo run --release
```

Paste CSV with case-insensitive `date`, `description`, and `amount` columns.
Dates may be `YYYY-MM-DD`, `MM/DD/YYYY`, or `DD/MM/YYYY`. Select whether your
bank represents charges as negative or positive amounts. A small fictional
sample is preloaded and also lives at [`examples/transactions.csv`](examples/transactions.csv).

## What it detects

- Weekly, every-two-weeks, monthly, quarterly, and yearly cadences.
- Merchant descriptions with changing statement reference numbers.
- Typical charge, next expected date, confidence, and estimated yearly cost.
- Price creep of at least 3% and 50 cents between the first and latest charge.

A series needs at least three observations. At least 70% of its intervals must
fit one cadence and at least 70% of its amounts must be within 20% of the median.
These transparent heuristics favor understandable candidates over opaque
classification.

## Verify it

The project supports stable Rust and declares Rust 1.88 as its MSRV.

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo +1.88.0 check --all-features
```

## Limitations

Bill Echo analyzes only the rows you paste. It does not connect to a bank,
identify cancellations, account for taxes or currency conversions, or promise
that a merchant will charge again. Merchant grouping is deliberately local and
rule-based; review every candidate against the original statement.

## License

[MIT](LICENSE)
