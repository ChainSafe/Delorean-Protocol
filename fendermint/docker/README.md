# Docker Build

The docker image for `fendermint` can be built with the following top-level command:

```bash
make docker-build
```

The image contains both the `fendermint` and `ipc-cli` executables (dispatched by [docker-entry.sh](./docker-entry.sh)), as well as all the actor bytecode that needs to be deployed at genesis inside a subnet. In other words, it's a one-stop image for everything IPC. 

## Dependencies

As a pre-requisite this will ensure that the following artifacts are prepared:
* The builtin-actors bundle is downloaded from GitHub; if you upgrade the version of these, make sure you clear out any existing artifacts or you might get interface version conflicts at runtime when the bundles are loaded.
* The custom actor bundle is built; this prepares some JSON artifacts as well, which are needed for deployment.
* The IPC actor bindings are generated; this is refreshed every time a Solidity artifact changes.

The actor bundles are CAR files which are copied into the `fendermint` image, so that we don't have to procure them separately when we want to create a container.

## Dockerfiles

The full Docker build comprises two stages: the builder stage and the runner stage.

- The builder stage builds Fendermint and ipc-cli. It relies on a heavier base image + dependencies required by the build.
- The runner stage picks the executables and places them on lighter base images satisfying dynamically linked library dependencies.

## Builder Stage

For the builder stage, there are two variants of the `Dockerfile`s one of which is chosen to perform the build depending on the `PROFILE` environment variable.

- `builder.ci.Dockerfile`: used only in CI when `PROFILE=ci`.
- `builder.local.Dockerfile`: used otherwise (e.g. for local builds).

In both cases the final `Dockerfile` consists of a builder stage variant, joined with the `runner.Dockerfile`.

### Local Build

`build.local.Dockerfile` uses simple `cargo install` with `--mount=type=cache` to take advantage of the durable caching available on the developer machine to speed up builds.

This kind of cache would not work on CI where a new machine does every build. It was also observed that with multiarch builds the cache seems to be invalidated by the different platforms.

### CI Build

`build.ci.Dockerfile` uses a multi-stage build in itself by first building only the dependencies with all the user code stripped away, to try to take advantage of caching provided by docker layers; then it builds the final application by restoring the user code.

> ⚠️ Unfortunately the caching relying on Docker layers on CI has failed to take into account the fact that only the final "runnable" image is published, not the builder, so the whole build has to be done from scratch each time. There is a [closed PR](https://github.com/consensus-shipyard/ipc/pull/699) that pushes the builder image but it's 10GB and exporting it on CI failed, let alone pushing it. It would be worth to investigate using [Github caching](https://docs.docker.com/build/ci/github-actions/cache/#github-cache) to speed up builds.

It is multiarch build to support both Linux and MacOS, depending on what `BUILDX_FLAGS` it is called with. The multiarch flags are set by the [fendermint-publish.yaml](../../.github/workflows/fendermint-publish.yaml) workflow; for the tests only the one matching the CI platform is built.

> ⚠️ Due to some problems encountered with the multiarch build the builder and the runner are using different base images: the builder is Ubuntu, the runner is Debian based. If there are problems with the versioning of system libraries during execution, it might be due to a mismatch in these releases, and a suitable tag needs to be found for both, e.g. not `latest` which might have different release schedules.
