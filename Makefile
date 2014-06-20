


all: bitcoin-rust wizards-wallet

bitcoin-rust: bitcoin/*.rs bitcoin/*/*.rs
	rustc --opt-level=0 --crate-type=rlib bitcoin/lib.rs

bitcoin-docs: bitcoin/*.rs bitcoin/*/*.rs
	rustdoc bitcoin/lib.rs

wizards-wallet: *.rs *.rlib
	rustc --opt-level=0 -L . wizards-wallet.rs

check: *.rs *.rlib bitcoin/*.rs bitcoin/*/*.rs
	rustc --test --crate-type=rlib bitcoin/lib.rs -o testbin
	./testbin
	rm testbin
	rustc --test -L . test-root.rs -o testbin
	./testbin
	rm testbin

