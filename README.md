# Sens-O-Matic

A backend API for coordinating spontaneous meetups with friends. It turns
"Who's up for drinks?" into an actual hangout with minimal friction: it
collects structured availability, finds the time overlaps algorithmically, and
reduces the back-and-forth messaging.

## Install

```bash
cargo install sens-o-matic
```

## Build and run

Requires Rust 1.93+ (edition 2024) and [just](https://github.com/casey/just).

```bash
just build
just run
```

The server listens on `http://localhost:3000`.

## More

The book has the concepts and the usage, with the Rust docs of `main`, at
<https://n8behavior.github.io/sens-o-matic/>. `just book` serves it with the
Rust docs and Swagger UI on your machine, and opens the browser (it needs
[mdbook](https://rust-lang.github.io/mdBook/)). `just` lists every command.

## License

This project is licensed under the [MIT License](LICENSE).
