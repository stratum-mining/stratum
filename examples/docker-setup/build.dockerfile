FROM rust:latest as builder

RUN git clone https://github.com/stratum-mining/stratum.git

WORKDIR /stratum

RUN git switch dev

RUN cargo build --release

# FROM alpine:latest

# WORKDIR /app

# COPY --from=builder /stratum/target/release  .

ENTRYPOINT ["/stratum/entrypoint.sh"]
