# USync

Try to send large bulks of data via UDP.
Raptorq was utlized as error correction code to achieve reliable transmission.

## Caveat

This is a personal toy project to transfer some files, as long as fullfill my curisority about whether UDP + Fountain
code is a pratical alternative method under bad network condition, thus no guarantee is provided.


The project was developed, tested on and supports UNIX-like system only for now, especially with regard of file system operations. Yet future support for Windows is planned.

## Example usage

1. Generate plan file:
```bash
cargo run --bin planner -- --file ~/test.zip > test.plan
```

2. Generate key pairs
```bash
cargo test protocol::wire::verify::tests::test_exchange_public_key -- --no-capture
```
Note: Signing key == Private key,   Verifying key == Public key


3. Run Server
```bash
cargo run --release --bin server -- --plan-file test.plan --listening 0.0.0.0:7234 --public-key pub.key --folder ~
```

4. Run Client
```bash
cargo run --release --bin client -- --plan-file plan.plan --server 127.0.0.1:7234 --private-key <YOUR-SIGNING-KEY>
```
