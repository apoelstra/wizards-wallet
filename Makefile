


all: bitcoin-rust wizards-wallet

bitcoin-rust: bitcoin/*.rs bitcoin/*/*.rs
	rustc --opt-level=3 -g --crate-type=rlib bitcoin/lib.rs

bitcoin-docs: bitcoin/*.rs bitcoin/*/*.rs
	rustdoc bitcoin/lib.rs

wizards-wallet: *.rs libbitcoin.rlib
	rustc --opt-level=3 -g -L . wizards-wallet.rs

check: *.rs libbitcoin.rlib bitcoin/*.rs bitcoin/*/*.rs
	rustc --test --crate-type=rlib bitcoin/lib.rs -o testbin
	./testbin
	rm testbin
	rustc --test -L . test-root.rs -o testbin
	./testbin
	rm testbin

