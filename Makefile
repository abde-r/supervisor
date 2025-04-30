build:
	@rustc -o taskmaster main.rs

run: build
	@taskmaster

clean:
	@rm taskmaster
