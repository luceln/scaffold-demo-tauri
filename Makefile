# scaffold-demo-tauri —— 常用命令入口。
#
# 这里每条 target 都只是"把已有命令起个短名字"，不含任何逻辑。
# 之所以还留着 Makefile：它是这个仓库里**唯一**一个跨语言、跨平台、
# 不用记参数顺序的入口 —— 你不用记住 `cargo clippy --manifest-path ... --all-targets`
# 这一长串，只记 `make check`。
#
# Windows 上没有 make 也不影响：README 的「不用 make 的等价命令」一节
# 给了逐条对照的原生命令。

.DEFAULT_GOAL := help
.PHONY: help install dev build check ci lint typecheck test fmt fmt-check clippy clean

help: ## 显示本帮助
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

ci: ## 本地完整 CI 检查面（与 .github/workflows/ci.yml 同一条命令）
	pnpm run ci

install: ## 安装前后端依赖
	pnpm install
	cd src-tauri && cargo fetch

dev: ## 启动开发模式（Vite + Tauri 热重载）
	pnpm tauri dev

build: ## 构建发布包（前端 dist + Rust 可执行文件/安装包）
	pnpm run build
	cargo build --manifest-path src-tauri/Cargo.toml --release

check: lint typecheck test fmt-check clippy ## 跑全部静态检查与测试

lint: ## 前端静态检查（含 no-console 裸打印闸门）
	pnpm run lint

typecheck: ## 前端类型检查
	pnpm run typecheck

test: ## 跑全部测试（前端 vitest + Rust 含 §7.2 黄金纵切）
	pnpm run test
	cargo test --manifest-path src-tauri/Cargo.toml

fmt: ## 格式化全部代码
	cargo fmt --manifest-path src-tauri/Cargo.toml --all
	pnpm run format

fmt-check: ## 只检查格式，不改文件（CI 用）
	cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check

clippy: ## Rust 静态检查（警告即失败；含裸打印闸门）
	cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

clean: ## 清掉构建产物
	rm -rf dist src-tauri/target
