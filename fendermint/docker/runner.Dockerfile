# syntax=docker/dockerfile:1

# The builder and runner are in separate Dockerfile so that we can use different caching strategies
# in the builder depending on whether we are building on CI or locally, but they are concatenated
# just before the build.

FROM debian:bookworm-slim

RUN apt-get update && \
  apt-get install -y libssl3 ca-certificates curl && \
  rm -rf /var/lib/apt/lists/*

ENV FM_HOME_DIR=/fendermint
ENV HOME=$FM_HOME_DIR
WORKDIR $FM_HOME_DIR

EXPOSE 26658
EXPOSE 8445
EXPOSE 9184

ENTRYPOINT ["docker-entry.sh"]
CMD ["run"]

STOPSIGNAL SIGTERM

ENV FM_ABCI__LISTEN__HOST=0.0.0.0
ENV FM_ETH__LISTEN__HOST=0.0.0.0
ENV FM_METRICS__LISTEN__HOST=0.0.0.0

RUN mkdir /fendermint/logs
RUN chmod 777 /fendermint/logs

COPY fendermint/docker/.artifacts/bundle.car $FM_HOME_DIR/bundle.car
COPY fendermint/docker/.artifacts/custom_actors_bundle.car $FM_HOME_DIR/custom_actors_bundle.car
COPY fendermint/docker/.artifacts/contracts $FM_HOME_DIR/contracts
COPY fendermint/docker/docker-entry.sh /usr/local/bin/docker-entry.sh
COPY --from=builder /app/fendermint/app/config $FM_HOME_DIR/config
COPY --from=builder /app/output/bin/fendermint /usr/local/bin/fendermint
COPY --from=builder /app/output/bin/ipc-cli /usr/local/bin/ipc-cli
