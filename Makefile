# Forge Makefile
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
GO_LAYOUT_MODE ?= root
GO_LAYOUT_GUARD := ./scripts/go-layout-guard.sh
RUST_FIRST ?= 0

# Binary names
BINARY_CLI := forge
BINARY_DAEMON := forged
BINARY_RUNNER := forge-agent-runner
BINARY_FMAIL := fmail
RUST_BINARY_CLI := rforge
RUST_BINARY_DAEMON := rforged
RUST_BINARY_FMAIL := rfmail

# Directories
BUILD_DIR := ./build
CMD_CLI := ./cmd/forge
CMD_DAEMON := ./cmd/forged
CMD_RUNNER := ./cmd/forge-agent-runner
CMD_FMAIL := ./cmd/fmail
RUST_DIR := ./rust

# Installation directories
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
GOBIN ?= $(shell go env GOPATH)/bin
RUST_INSTALL_DIR ?= $(GOBIN)

# Platforms for cross-compilation
PLATFORMS := linux/amd64 linux/arm64 darwin/amd64 darwin/arm64

.PHONY: all build go-layout-guard build-cli build-daemon build-runner build-fmail build-rust build-rust-cli build-rust-daemon build-rust-fmail clean test lint fmt vet tidy install install-local install-system install-rust install-rust-system uninstall uninstall-rust uninstall-rust-system dev help proto proto-lint rust-daemon-runtime-parity
.PHONY: perf-smoke perf-bench

# Default target
all: build

## Build targets

# Build both binaries
build: go-layout-guard build-cli build-daemon build-runner build-fmail

# Guard against partial Go source tree moves during rust rewrite staging.
go-layout-guard:
	@$(GO_LAYOUT_GUARD) --repo-root . --mode $(GO_LAYOUT_MODE)

# Build the CLI/TUI binary
build-cli:
	@echo "Building $(BINARY_CLI)..."
	@mkdir -p $(BUILD_DIR)
ifeq ($(RUST_FIRST),1)
	@echo "RUST_FIRST=1: using Rust $(RUST_BINARY_CLI) as $(BINARY_CLI)"
	@cd $(RUST_DIR) && cargo build --release -p forge-cli --bin $(RUST_BINARY_CLI)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_CLI) $(BUILD_DIR)/$(BINARY_CLI)
else
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_CLI) $(CMD_CLI)
endif

# Build the daemon binary
build-daemon:
	@echo "Building $(BINARY_DAEMON)..."
	@mkdir -p $(BUILD_DIR)
ifeq ($(RUST_FIRST),1)
	@echo "RUST_FIRST=1: using Rust $(RUST_BINARY_DAEMON) as $(BINARY_DAEMON)"
	@cd $(RUST_DIR) && cargo build --release -p forge-daemon --bin $(RUST_BINARY_DAEMON)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_DAEMON) $(BUILD_DIR)/$(BINARY_DAEMON)
else
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_DAEMON) $(CMD_DAEMON)
endif

# Build the agent runner binary
build-runner:
	@echo "Building $(BINARY_RUNNER)..."
	@mkdir -p $(BUILD_DIR)
ifeq ($(RUST_FIRST),1)
	@echo "RUST_FIRST=1: using Rust $(BINARY_RUNNER)"
	@cd $(RUST_DIR) && cargo build --release -p forge-runner --bin $(BINARY_RUNNER)
	@cp $(RUST_DIR)/target/release/$(BINARY_RUNNER) $(BUILD_DIR)/$(BINARY_RUNNER)
else
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_RUNNER) $(CMD_RUNNER)
endif

# Build the fmail binary
build-fmail:
	@echo "Building $(BINARY_FMAIL)..."
	@mkdir -p $(BUILD_DIR)
ifeq ($(RUST_FIRST),1)
	@echo "RUST_FIRST=1: using Rust $(RUST_BINARY_FMAIL) as $(BINARY_FMAIL)"
	@cd $(RUST_DIR) && cargo build --release -p fmail-cli --bin $(RUST_BINARY_FMAIL)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_FMAIL) $(BUILD_DIR)/$(BINARY_FMAIL)
else
	$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_FMAIL) $(CMD_FMAIL)
endif

# Build for all platforms
build-all:
ifeq ($(RUST_FIRST),1)
	@echo "build-all does not yet support RUST_FIRST=1; use make build RUST_FIRST=1" && exit 1
else
	@for platform in $(PLATFORMS); do \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_CLI)-$${platform%/*}-$${platform#*/} $(CMD_CLI); \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_DAEMON)-$${platform%/*}-$${platform#*/} $(CMD_DAEMON); \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_RUNNER)-$${platform%/*}-$${platform#*/} $(CMD_RUNNER); \
		GOOS=$${platform%/*} GOARCH=$${platform#*/} \
		$(GOBUILD) $(LDFLAGS) -o $(BUILD_DIR)/$(BINARY_FMAIL)-$${platform%/*}-$${platform#*/} $(CMD_FMAIL); \
	done
endif

## Rust build targets (side-by-side; non-conflicting binaries)

build-rust: build-rust-cli build-rust-daemon build-rust-fmail

build-rust-cli:
	@echo "Building $(RUST_BINARY_CLI) (Rust)..."
	@mkdir -p $(BUILD_DIR)
	@cd $(RUST_DIR) && cargo build --release -p forge-cli --bin $(RUST_BINARY_CLI)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_CLI) $(BUILD_DIR)/$(RUST_BINARY_CLI)

build-rust-daemon:
	@echo "Building $(RUST_BINARY_DAEMON) (Rust)..."
	@mkdir -p $(BUILD_DIR)
	@cd $(RUST_DIR) && cargo build --release -p forge-daemon --bin $(RUST_BINARY_DAEMON)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_DAEMON) $(BUILD_DIR)/$(RUST_BINARY_DAEMON)

build-rust-fmail:
	@echo "Building $(RUST_BINARY_FMAIL) (Rust)..."
	@mkdir -p $(BUILD_DIR)
	@cd $(RUST_DIR) && cargo build --release -p fmail-cli --bin $(RUST_BINARY_FMAIL)
	@cp $(RUST_DIR)/target/release/$(RUST_BINARY_FMAIL) $(BUILD_DIR)/$(RUST_BINARY_FMAIL)

## Development targets

# Run the CLI in development mode
dev:
	@$(GOCMD) run $(CMD_CLI)

## Installation targets

# Install to GOPATH/bin (default, no sudo required)
install: build
	@echo "Installing to $(GOBIN)..."
	@mkdir -p $(GOBIN)
	@install -m 755 $(BUILD_DIR)/$(BINARY_CLI) $(GOBIN)/.$(BINARY_CLI).tmp
	@mv $(GOBIN)/.$(BINARY_CLI).tmp $(GOBIN)/$(BINARY_CLI)
	@install -m 755 $(BUILD_DIR)/$(BINARY_DAEMON) $(GOBIN)/.$(BINARY_DAEMON).tmp
	@mv $(GOBIN)/.$(BINARY_DAEMON).tmp $(GOBIN)/$(BINARY_DAEMON)
	@install -m 755 $(BUILD_DIR)/$(BINARY_RUNNER) $(GOBIN)/.$(BINARY_RUNNER).tmp
	@mv $(GOBIN)/.$(BINARY_RUNNER).tmp $(GOBIN)/$(BINARY_RUNNER)
	@install -m 755 $(BUILD_DIR)/$(BINARY_FMAIL) $(GOBIN)/.$(BINARY_FMAIL).tmp
	@mv $(GOBIN)/.$(BINARY_FMAIL).tmp $(GOBIN)/$(BINARY_FMAIL)
	@echo "Installed $(BINARY_CLI), $(BINARY_DAEMON), $(BINARY_RUNNER), and $(BINARY_FMAIL) to $(GOBIN)"
	@echo ""
	@echo "Make sure $(GOBIN) is in your PATH:"
	@echo "  export PATH=\"\$$PATH:$(GOBIN)\""

# Alias for install
install-local: install

# Install system-wide (requires sudo)
install-system: build
	@echo "Installing to $(BINDIR) (may require sudo)..."
	@mkdir -p $(BINDIR)
	@install -m 755 $(BUILD_DIR)/$(BINARY_CLI) $(BINDIR)/$(BINARY_CLI)
	@install -m 755 $(BUILD_DIR)/$(BINARY_DAEMON) $(BINDIR)/$(BINARY_DAEMON)
	@install -m 755 $(BUILD_DIR)/$(BINARY_RUNNER) $(BINDIR)/$(BINARY_RUNNER)
	@install -m 755 $(BUILD_DIR)/$(BINARY_FMAIL) $(BINDIR)/$(BINARY_FMAIL)
	@echo "Installed $(BINARY_CLI), $(BINARY_DAEMON), $(BINARY_RUNNER), and $(BINARY_FMAIL) to $(BINDIR)"

install-rust: build-rust
	@echo "Installing Rust side-by-side binaries to $(RUST_INSTALL_DIR)..."
	@mkdir -p $(RUST_INSTALL_DIR)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_CLI) $(RUST_INSTALL_DIR)/.$(RUST_BINARY_CLI).tmp
	@mv $(RUST_INSTALL_DIR)/.$(RUST_BINARY_CLI).tmp $(RUST_INSTALL_DIR)/$(RUST_BINARY_CLI)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_DAEMON) $(RUST_INSTALL_DIR)/.$(RUST_BINARY_DAEMON).tmp
	@mv $(RUST_INSTALL_DIR)/.$(RUST_BINARY_DAEMON).tmp $(RUST_INSTALL_DIR)/$(RUST_BINARY_DAEMON)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_FMAIL) $(RUST_INSTALL_DIR)/.$(RUST_BINARY_FMAIL).tmp
	@mv $(RUST_INSTALL_DIR)/.$(RUST_BINARY_FMAIL).tmp $(RUST_INSTALL_DIR)/$(RUST_BINARY_FMAIL)
	@echo "Installed $(RUST_BINARY_CLI), $(RUST_BINARY_DAEMON), and $(RUST_BINARY_FMAIL) to $(RUST_INSTALL_DIR)"

install-rust-system: build-rust
	@echo "Installing Rust side-by-side binaries to $(BINDIR) (may require sudo)..."
	@mkdir -p $(BINDIR)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_CLI) $(BINDIR)/$(RUST_BINARY_CLI)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_DAEMON) $(BINDIR)/$(RUST_BINARY_DAEMON)
	@install -m 755 $(BUILD_DIR)/$(RUST_BINARY_FMAIL) $(BINDIR)/$(RUST_BINARY_FMAIL)
	@echo "Installed $(RUST_BINARY_CLI), $(RUST_BINARY_DAEMON), and $(RUST_BINARY_FMAIL) to $(BINDIR)"

# Uninstall from GOPATH/bin
uninstall:
	@echo "Removing from $(GOBIN)..."
	@rm -f $(GOBIN)/$(BINARY_CLI)
	@rm -f $(GOBIN)/$(BINARY_DAEMON)
	@rm -f $(GOBIN)/$(BINARY_RUNNER)
	@rm -f $(GOBIN)/$(BINARY_FMAIL)
	@echo "Removed $(BINARY_CLI), $(BINARY_DAEMON), $(BINARY_RUNNER), and $(BINARY_FMAIL) from $(GOBIN)"

uninstall-rust:
	@echo "Removing Rust side-by-side binaries from $(RUST_INSTALL_DIR)..."
	@rm -f $(RUST_INSTALL_DIR)/$(RUST_BINARY_CLI)
	@rm -f $(RUST_INSTALL_DIR)/$(RUST_BINARY_DAEMON)
	@rm -f $(RUST_INSTALL_DIR)/$(RUST_BINARY_FMAIL)
	@echo "Removed $(RUST_BINARY_CLI), $(RUST_BINARY_DAEMON), and $(RUST_BINARY_FMAIL) from $(RUST_INSTALL_DIR)"

# Uninstall from system
uninstall-system:
	@echo "Removing from $(BINDIR) (may require sudo)..."
	@rm -f $(BINDIR)/$(BINARY_CLI)
	@rm -f $(BINDIR)/$(BINARY_DAEMON)
	@rm -f $(BINDIR)/$(BINARY_RUNNER)
	@rm -f $(BINDIR)/$(BINARY_FMAIL)
	@echo "Removed $(BINARY_CLI), $(BINARY_DAEMON), $(BINARY_RUNNER), and $(BINARY_FMAIL) from $(BINDIR)"

uninstall-rust-system:
	@echo "Removing Rust side-by-side binaries from $(BINDIR) (may require sudo)..."
	@rm -f $(BINDIR)/$(RUST_BINARY_CLI)
	@rm -f $(BINDIR)/$(RUST_BINARY_DAEMON)
	@rm -f $(BINDIR)/$(RUST_BINARY_FMAIL)
	@echo "Removed $(RUST_BINARY_CLI), $(RUST_BINARY_DAEMON), and $(RUST_BINARY_FMAIL) from $(BINDIR)"

# Install using go install (builds and installs in one step)
go-install:
	@echo "Installing $(BINARY_CLI) via go install..."
	$(GOCMD) install $(LDFLAGS) $(CMD_CLI)
	@echo "Installing $(BINARY_DAEMON) via go install..."
	$(GOCMD) install $(LDFLAGS) $(CMD_DAEMON)
	@echo "Installing $(BINARY_RUNNER) via go install..."
	$(GOCMD) install $(LDFLAGS) $(CMD_RUNNER)
	@echo "Installing $(BINARY_FMAIL) via go install..."
	$(GOCMD) install $(LDFLAGS) $(CMD_FMAIL)
	@echo "Installed to $(GOBIN)"

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

# Run Rust daemon runtime parity bring-up suite.
rust-daemon-runtime-parity:
	@scripts/rust-daemon-runtime-parity.sh

## Perf targets (gated; opt-in via build tags)

# Perf smoke: runs fast-ish budget checks on a synthetic fmail mailbox.
perf-smoke:
	@echo "Running fmail TUI perf smoke (tags=perf)..."
	@env -u GOROOT -u GOTOOLDIR $(GOTEST) -tags=perf ./internal/fmailtui/... -run TestPerfSmokeBudgets -count=1

# Perf benchmarks: captures baseline numbers for hot paths (provider/search).
perf-bench:
	@echo "Running fmail TUI perf benchmarks (tags=perf)..."
	@env -u GOROOT -u GOTOOLDIR $(GOTEST) -tags=perf ./internal/fmailtui/... -run '^$$' -bench Perf -benchmem -count=1

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

## Protocol Buffers

# Generate protobuf code
proto:
	@echo "Generating protobuf code..."
	@which buf > /dev/null || (echo "buf not installed. Run: go install github.com/bufbuild/buf/cmd/buf@latest" && exit 1)
	buf generate
	@echo "Generated code in gen/"

# Lint protobuf files
proto-lint:
	@echo "Linting protobuf files..."
	@which buf > /dev/null || (echo "buf not installed. Run: go install github.com/bufbuild/buf/cmd/buf@latest" && exit 1)
	buf lint

# Update buf dependencies
proto-deps:
	@echo "Updating buf dependencies..."
	buf dep update

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
	@echo "Forge - Control plane for AI coding agents"
	@echo ""
	@echo "Usage:"
	@echo "  make [target]"
	@echo ""
	@echo "Build Targets:"
	@echo "  build          Build both CLI and daemon binaries to ./build/"
	@echo "  go-layout-guard Validate expected Go source layout for the current migration stage"
	@echo "  build-cli      Build only the CLI/TUI binary"
	@echo "  build-daemon   Build only the daemon binary"
	@echo "  build-all      Build for all platforms (cross-compile)"
	@echo "  build-rust     Build Rust side-by-side binaries to ./build/ (rforge, rforged, rfmail)"
	@echo "  clean          Remove build artifacts"
	@echo ""
	@echo "Install Targets:"
	@echo "  install        Build and install to GOPATH/bin (recommended)"
	@echo "  install-local  Alias for install"
	@echo "  install-system Build and install to /usr/local/bin (requires sudo)"
	@echo "  install-rust   Build and install Rust side-by-side binaries to RUST_INSTALL_DIR (default: GOPATH/bin)"
	@echo "  install-rust-system Build and install Rust side-by-side binaries to /usr/local/bin (requires sudo)"
	@echo "  go-install     Use 'go install' directly"
	@echo "  uninstall      Remove from GOPATH/bin"
	@echo "  uninstall-system Remove from /usr/local/bin (requires sudo)"
	@echo "  uninstall-rust Remove Rust side-by-side binaries from RUST_INSTALL_DIR"
	@echo "  uninstall-rust-system Remove Rust side-by-side binaries from /usr/local/bin (requires sudo)"
	@echo ""
	@echo "Development Targets:"
	@echo "  dev            Run the CLI without building"
	@echo "  test           Run all tests with race detector"
	@echo "  test-coverage  Run tests with HTML coverage report"
	@echo "  test-short     Run short tests only"
	@echo "  rust-daemon-runtime-parity Run Rust daemon runtime parity bring-up suite"
	@echo "  lint           Run golangci-lint"
	@echo "  fmt            Format code with gofmt"
	@echo "  vet            Run go vet"
	@echo "  tidy           Tidy go.mod dependencies"
	@echo "  check          Run all checks (fmt, vet, lint, test)"
	@echo ""
	@echo "Protobuf Targets:"
	@echo "  proto          Generate protobuf code"
	@echo "  proto-lint     Lint protobuf files"
	@echo "  proto-deps     Update buf dependencies"
	@echo ""
	@echo "Quick Start:"
	@echo "  make build                    # Build to ./build/"
	@echo "  make build RUST_FIRST=1       # Build default binaries from Rust targets"
	@echo "  make install                  # Build + install to GOPATH/bin"
	@echo "  sudo make install-system      # Build + install to /usr/local/bin"
	@echo ""
	@echo "Variables (override with VAR=value):"
	@echo "  VERSION        $(VERSION)"
	@echo "  COMMIT         $(COMMIT)"
	@echo "  PREFIX         $(PREFIX)"
	@echo "  BINDIR         $(BINDIR)"
	@echo "  GOBIN          $(GOBIN)"
	@echo "  RUST_INSTALL_DIR $(RUST_INSTALL_DIR)"
	@echo "  GO_LAYOUT_MODE $(GO_LAYOUT_MODE)"
	@echo "  RUST_FIRST     $(RUST_FIRST)"
