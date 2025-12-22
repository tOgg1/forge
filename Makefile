# Swarm Makefile
# Control plane for AI coding agents

# Build variables
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")
COMMIT ?= $(shell git rev-parse --short HEAD 2>/dev/null || echo "none")
DATE ?= $(shell date -u +"%Y-%m-%dT%H:%M:%SZ")
LDFLAGS := -ldflags "-X main.version=$(VERSION) -X main.commit=$(COMMIT) -X main.date=$(DATE)"

# Go variables
GOCMD := go
GOBUILD := $(GOCMD) build
GOTEST := $(GOCMD) test
GOVET := $(GOCMD) vet
GOFMT := gofmt
GOMOD := $(GOCMD) mod

# Binary names
BINARY_CLI := swarm
BINARY_DAEMON := swarmd

# Directories
BUILD_DIR := ./build
CMD_CLI := ./cmd/swarm
CMD_DAEMON := ./cmd/swarmd

# Platforms for cross-compilation
PLATFORMS := linux/amd64 linux/arm64 darwin/amd64 darwin/arm64

.PHONY: all build build-cli build-daemon clean test lint fmt vet tidy install dev help

# Default target
all: build

## Build targets

# Build both binaries
build: build-cli build-daemon

# Build the CLI/TUI binary
build-cli:
	@echo "Building $(BINARY_CLI)..."
	@mkdir -p $(BUILD_DIR)
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_CLI) $(CMD_CLI)

# Build the daemon binary
build-daemon:
	@echo "Building $(BINARY_DAEMON)..."
	@mkdir -p $(BUILD_DIR)
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_DAEMON) $(CMD_DAEMON)

# Build for all platforms
build-all:
	@for platform in $(PLATFORMS); do \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_CLI)-$${platform%/*}-$${platform#*/} $(CMD_CLI); \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_DAEMON)-$${platform%/*}-$${platform#*/} $(CMD_DAEMON); \
	done

## Development targets

# Run the CLI in development mode
dev:
	@$(GOCMD) run $(CMD_CLI)

# Install to GOPATH/bin
install:
	@echo "Installing $(BINARY_CLI)..."
	$(GOCMD) install $(LDFLAGS) $(CMD_CLI)
	@echo "Installing $(BINARY_DAEMON)..."
	$(GOCMD) install $(LDFLAGS) $(CMD_DAEMON)

## Test targets

# Run all tests
test:
	@echo "Running tests..."
	$(GOTEST) -v -race -cover ./...

# Run tests with coverage report
test-coverage:
	@echo "Running tests with coverage..."
	@mkdir -p $(BUILD_DIR)
	$(GOTEST) -v -race -coverprofile=$(BUILD_DIR)/coverage.out ./...
	$(GOCMD) tool cover -html=$(BUILD_DIR)/coverage.out -o $(BUILD_DIR)/coverage.html
	@echo "Coverage report: $(BUILD_DIR)/coverage.html"

# Run short tests only
test-short:
	$(GOTEST) -v -short ./...

## Code quality targets

# Run linter (requires golangci-lint)
lint:
	@echo "Running linter..."
	@which golangci-lint > /dev/null || (echo "golangci-lint not installed. Run: go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest" && exit 1)
	golangci-lint run ./...

# Format code
fmt:
	@echo "Formatting code..."
	$(GOFMT) -s -w .

# Check formatting
fmt-check:
	@echo "Checking formatting..."
	@test -z "$$($(GOFMT) -l .)" || (echo "Code is not formatted. Run 'make fmt'" && exit 1)

# Run go vet
vet:
	@echo "Running vet..."
	$(GOVET) ./...

# Tidy dependencies
tidy:
	@echo "Tidying dependencies..."
	$(GOMOD) tidy

# Run all checks (for CI)
check: fmt-check vet lint test

## Cleanup

# Clean build artifacts
clean:
	@echo "Cleaning..."
	rm -rf $(BUILD_DIR)
	$(GOCMD) clean -cache -testcache

## Database

# Run database migrations
migrate-up:
	@echo "Running migrations..."
	@echo "TODO: Implement migrations"

migrate-down:
	@echo "Rolling back migrations..."
	@echo "TODO: Implement migrations"

## Help

# Show help
help:
	@echo "Swarm - Control plane for AI coding agents"
	@echo ""
	@echo "Usage:"
	@echo "  make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build         Build both CLI and daemon binaries"
	@echo "  build-cli     Build the CLI/TUI binary"
	@echo "  build-daemon  Build the daemon binary"
	@echo "  build-all     Build for all platforms"
	@echo "  dev           Run the CLI in development mode"
	@echo "  install       Install binaries to GOPATH/bin"
	@echo "  test          Run all tests"
	@echo "  test-coverage Run tests with coverage report"
	@echo "  lint          Run golangci-lint"
	@echo "  fmt           Format code"
	@echo "  vet           Run go vet"
	@echo "  tidy          Tidy dependencies"
	@echo "  check         Run all checks (CI)"
	@echo "  clean         Clean build artifacts"
	@echo "  help          Show this help"
	@echo ""
	@echo "Variables:"
	@echo "  VERSION=$(VERSION)"
	@echo "  COMMIT=$(COMMIT)"
