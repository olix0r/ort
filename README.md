# ort: Oliver's Routine Tests

The project consists of a client and server that are ready to be deployed in
Kubernetes, especially for testing [Linkerd](https://linkerd.io).

```
ort 0.1.13
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
## Upgrade the toplogy with a custom setup
:; helm upgrade ort ./chart --namespace ort \
    --set load.threads=5 \
    --set load.flags.concurrencyLimit=10 \
    --set load.flags.requestLimit=300 \
    --set server.services=3 \
    --set linkerd.config.proxyLogLevel=linkerd=debug\\,info \
    --set linkerd.config.proxyImage=ghcr.io/olix0r/l2-proxy \
    --set linkerd.config.proxyVersion=detect.0c823a6a
## Get the Grafana addresst
:; echo "$(kubectl get -n ort-viz svc/grafana -o jsonpath='{.status.loadBalancer.ingress[0].ip}'):3000"
172.23.0.2:3000
```
See <./chart/values.yml> and  <./viz/values.yml>

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
