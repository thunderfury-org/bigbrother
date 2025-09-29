FROM golang:1.25 as builder
WORKDIR /app
COPY go.* ./
RUN go mod download
COPY . .
RUN make build

FROM alpine:latest
RUN apk --no-cache add ca-certificates
WORKDIR /app
COPY --from=builder /app/bin/bigbrother ./
VOLUME ["/app/data"]
ENTRYPOINT ["./bigbrother", "server"]
