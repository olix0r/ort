```
:; mkdir reports
:; k3d cluster create ortiofay --volume $PWD/reports:/var/run/ortiofay
:; kubectl apply -f reports.yml
:; kubectl apply -f server.yml
:; kubectl apply -f load.yml
```
