FROM golang:1.25 AS builder
WORKDIR /app
COPY go.* ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 make build

FROM alpine:latest
RUN apk --no-cache add ca-certificates
WORKDIR /app
COPY --from=builder /app/bin/bigbrother ./
VOLUME ["/app/data"]
ENTRYPOINT ["./bigbrother", "server"]
