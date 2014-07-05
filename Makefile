


all: bitcoin-rust wizards-wallet

bitcoin-rust: bitcoin/*.rs bitcoin/*/*.rs
	rustc --opt-level=3 --crate-type=rlib bitcoin/lib.rs

bitcoin-docs: bitcoin/*.rs bitcoin/*/*.rs
	rustdoc bitcoin/lib.rs

wizards-wallet: *.rs libbitcoin-c7b18f3c-0.1-pre.rlib
	rustc --opt-level=3 -L . wizards-wallet.rs

check: *.rs libbitcoin-c7b18f3c-0.1-pre.rlib bitcoin/*.rs bitcoin/*/*.rs
	rustc --test --crate-type=rlib bitcoin/lib.rs -o testbin
	./testbin
	rm testbin
	rustc --test -L . test-root.rs -o testbin
	./testbin
	rm testbin

