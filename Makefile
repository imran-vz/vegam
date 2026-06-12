.PHONY: help check check-tauri install build-desktop dev

help:
	@echo "Development Targets:"
	@echo "  check                - Check dependencies and build"
	@echo "  install              - Install dependencies"
	@echo "  build-desktop        - Build desktop app"
	@echo "  dev                  - Run desktop app in development mode"
	@echo ""

check-tauri:
	cd src-tauri && cargo check

install:
	pnpm install

check: install check-tauri

build-desktop:
	pnpm tauri build

dev:
	pnpm tauri dev
