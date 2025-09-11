APP_NAME := bigbrother

.PHONY: all
all: build

.PHONY: build
build:
	go build -v -o ./bin/ ./cmd/...

.PHONY: test
test:
	go test -race -covermode=atomic -coverprofile=coverage.txt ./...

.PHONY: fmt
fmt:
	go fmt ./...

.PHONY: lint
lint:
	go vet ./...
	staticcheck ./...

.PHONY: clean
clean:
	rm -rf ./bin
