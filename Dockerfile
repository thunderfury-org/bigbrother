FROM rust:1-slim-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin bigbrother

FROM debian:bookworm-slim
WORKDIR /app

# 安装运行时必要的库（Rust 应用通常需要这些来处理 HTTPS 和基础库）
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# 创建非 root 用户（出于安全考虑）
RUN addgroup --gid 1001 appuser && \
    adduser --gid 1001 --uid 1001 appuser
RUN mkdir -p /app/data && chown -R appuser:appuser /app

# 从编译阶段拷贝二进制文件
COPY --from=builder --chown=appuser:appuser /app/target/release/bigbrother ./

USER appuser

# 数据卷挂载
VOLUME ["/app/data"]

# 运行命令
CMD ["/app/bigbrother", "server"]
