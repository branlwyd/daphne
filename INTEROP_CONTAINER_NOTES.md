To build, from the root directory of the repository run:

```
$ docker build . --tag daphne_interop
```

To run:

```
$ docker run -P daphne_interop
```

(the `-P` flag causes the exposed ports, e.g. 8788, to be mapped to the host
machine's ports. Note that the host port is likely different from 8788; use
`docker container ls` to see port mappings for running containers.)