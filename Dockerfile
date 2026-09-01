FROM --platform=$BUILDPLATFORM node:24-alpine AS web-builder
RUN npm install -g pnpm@11
WORKDIR /web
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml web/.npmrc ./
RUN pnpm install --frozen-lockfile
COPY web ./
RUN pnpm build

FROM rust:1-alpine AS chef
RUN apk add --no-cache musl-dev gcc g++ make cmake perl
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
COPY --from=web-builder /web/dist /app/web/dist
RUN cargo build --release --bin bigbrother

FROM alpine:3.20
WORKDIR /app

# 安装运行时必要的库（HTTPS 证书与时区数据）
RUN apk add --no-cache ca-certificates tzdata

# 创建非 root 用户（出于安全考虑）
RUN addgroup -g 1001 appuser && \
    adduser -D -u 1001 -G appuser appuser
RUN mkdir -p /app/data && chown -R appuser:appuser /app

# 从编译阶段拷贝二进制文件
COPY --from=builder --chown=appuser:appuser /app/target/release/bigbrother ./

USER appuser

# 数据卷挂载
VOLUME ["/app/data"]

# 运行命令
CMD ["/app/bigbrother", "server"]
