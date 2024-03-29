# ort: Oliver's Routine Tests

The project consists of a client and server that are ready to be deployed in
Kubernetes, especially for testing [Linkerd](https://linkerd.io).

```text
ort 0.2.11
Load harness

USAGE:
    ort [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -h, --help                 Print help information
        --threads <THREADS>    
    -V, --version              Print version information

SUBCOMMANDS:
    help      Print this message or the help of the given subcommand(s)
    load      Load generator
    server    Load target
```

## Running in Kubernetes

```sh
## Create a cluster (if necessary)
:; k3d cluster create
## Install a minimal Linkerd control plane
:; linkerd install --config=./linkerd.yaml |kubectl apply -f -
## Run a viz stack with a default dashboard
:; helm install ort-viz ./viz --create-namespace -n ort-viz
## Run a topology
:; helm install ort ./chart --create-namespace -n ort
## Upgrade the topology with a custom setup
:; helm upgrade ort ./chart --namespace ort \
    --set load.threads=5 \
    --set load.flags.concurrencyLimit=10 \
    --set load.flags.requestLimit=300 \
    --set server.services=3 \
    --set linkerd.config.proxyLogLevel=linkerd=debug\\,info \
    --set linkerd.config.proxyImage=ghcr.io/olix0r/l2-proxy \
    --set linkerd.config.proxyVersion=detect.0c823a6a
## Get the Grafana address
:; echo "$(kubectl get -n ort-viz svc/grafana -o jsonpath='{.status.loadBalancer.ingress[0].ip}'):3000"
172.23.0.2:3000
```

See <./chart/values.yml> and  <./viz/values.yml>

## Building images

```
:; docker buildx build . --platform=linux/amd64,linux/arm64 -t ghcr.io/olix0r/ort:v0.2.11 --push
```

## License

Copyright 2021 Oliver Gould @olix0r

Licensed under the Apache License, Version 2.0 (the "License"); you may not
use this file except in compliance with the License. You may obtain a copy of
the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations under
the License.
