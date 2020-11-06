# ort: Oliver's Routine Tests

The project consists of a client and server that are ready to be deployed in
Kubernetes, especially for testing [Linkerd](https://linkerd.io).

```
ort 0.1.7
Load harness

USAGE:
    ort <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    help      Prints this message or the help of the given subcommand(s)
    load      Load generator
    server    Load target
```

## Compiling

```sh
:; cargo check
:; cargo test
:; cargo build --release
:; docker buildx build .
```

## Running

```sh
:; RUST_LOG=ort=debug cargo run -- server
:; RUST_LOG=ort=debug cargo run -- load grpc://localhost:8070 http://localhost:8080
```

## Deploying

```sh
:; helm install ort . --namespace ort --create-namespace
```

See the <./values.yml>

## License

Copyright 2020 Oliver Gould @olix0r

Licensed under the Apache License, Version 2.0 (the "License"); you may not
use this file except in compliance with the License. You may obtain a copy of
the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations under
the License.
