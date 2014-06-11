


all: bitcoin-rust wizards-wallet

bitcoin-rust: bitcoin/*.rs bitcoin/*/*.rs
	rustc --crate-type=rlib bitcoin/lib.rs

wizards-wallet: *.rs *.rlib
	rustc -L . wizards-wallet.rs

check: *.rs *.rlib bitcoin/*.rs bitcoin/*/*.rs
	rustc --test --crate-type=rlib bitcoin/lib.rs -o testbin
	./testbin
	rm testbin
	rustc --test -L . wizards-wallet.rs -o testbin
	./testbin
	rm testbin

