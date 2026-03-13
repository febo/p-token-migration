include scripts/vars.env

nightly = +$(subst ",,${RUST_TOOLCHAIN_NIGHTLY})

clean:
	cargo clean

clean-ledger:
	@rm -rf ./target/migration-ledger

clippy:
	cargo $(nightly) clippy

format:
	cargo $(nightly) fmt --check

build: build-cli build-p-token build-programs
	@echo "✅ All set - run 'make run' to start the simulation."

build-cli:
	cargo build --release --manifest-path cli/Cargo.toml

build-p-token:
	@if [ ! -d target/token ]; then \
		git clone https://github.com/solana-program/token.git target/token; \
		git -C target/token checkout p-token@v1.0.0-rc.1; \
	fi
	@cd target/token && ARGS="--tools-version v1.54" make build-sbf-pinocchio-program
	@mkdir -p target/elfs
	@cp target/token/target/deploy/pinocchio_token_program.so target/elfs/p_token.so


build-programs:
	@cargo build-sbf --manifest-path programs/activator/Cargo.toml --features sbf-entrypoint --tools-version v1.54
	@mkdir -p target/elfs
	@cp target/deploy/cbmt_program_activator.so target/elfs/cbmt_program_activator.so

run:
	@./target/release/simulate