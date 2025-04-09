FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY target/release/mbzlists-resolvers /usr/local/bin/mbzlists-resolvers

EXPOSE 8888

ENTRYPOINT ["/usr/local/bin/mbzlists-resolvers"]
