FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY board ./board
COPY api ./api
RUN cargo build --release --locked --bin overseer

FROM debian:bookworm-slim
ARG CLAUDE_CODE_VERSION=latest
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates git tmux nodejs npm procps tini \
 && npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
 && npm cache clean --force \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --create-home --uid 10001 --shell /bin/sh runner
COPY --from=build /src/target/release/overseer /usr/local/bin/overseer
USER 10001
WORKDIR /home/runner
ENV SHELL=/bin/sh \
    OVERSEER_STATE_DIR=/home/runner/.local/state/overseer
RUN mkdir -p -m 700 "$OVERSEER_STATE_DIR" /home/runner/.claude /home/runner/repos /home/runner/worktrees \
 && overseer hooks install --apply
VOLUME /home/runner
ENTRYPOINT ["tini", "--"]
CMD ["sh", "-c", "overseer daemon --interval 5 & exec overseer sync"]
