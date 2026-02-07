# Nanobot Makefile

.PHONY: build run test clean fmt lint check doc

# 默认目标
.DEFAULT_GOAL := build

# 构建项目
build:
	cargo build --release

# 开发构建
dev:
	cargo build

# 运行 Agent 模式
agent:
	cargo run -- agent

# 运行 Gateway
gateway:
	cargo run -- gateway

# 查看状态
status:
	cargo run -- status

# 初始化配置
init:
	cargo run -- init

# 运行测试
test:
	cargo test

# 运行测试并显示输出
test-verbose:
	cargo test -- --nocapture

# 格式化代码
fmt:
	cargo fmt

# 检查代码
lint:
	cargo clippy -- -D warnings

# 类型检查
check:
	cargo check

# 生成文档
doc:
	cargo doc --open

# 清理构建产物
clean:
	cargo clean

# 安装到系统
install:
	cargo install --path .

# 运行并加载 .env 文件
run-env:
	@if [ -f .env ]; then \
		export $$(cat .env | grep -v '^#' | xargs) && cargo run -- agent; \
	else \
		echo "请先创建 .env 文件，参考 .env.example"; \
	fi

# 帮助信息
help:
	@echo "可用命令："
	@echo "  make build       - 构建发布版本"
	@echo "  make dev         - 开发构建"
	@echo "  make agent       - 运行 Agent 模式"
	@echo "  make gateway     - 运行 Gateway"
	@echo "  make status      - 查看状态"
	@echo "  make init        - 初始化配置"
	@echo "  make test        - 运行测试"
	@echo "  make fmt         - 格式化代码"
	@echo "  make lint        - 代码检查"
	@echo "  make check       - 类型检查"
	@echo "  make clean       - 清理构建产物"
	@echo "  make install     - 安装到系统"
	@echo "  make run-env     - 加载 .env 后运行"
