# egui-tao

[![Latest version](https://img.shields.io/crates/v/egui-winit.svg)](https://crates.io/crates/egui-winit)
[![Documentation](https://docs.rs/egui-winit/badge.svg)](https://docs.rs/egui-winit)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

This crates provides bindings between [`egui`](https://github.com/emilk/egui) and [`tao`](https://crates.io/crates/tao).

The library translates winit events to egui, handled copy/paste, updates the cursor, open links clicked in egui, etc.

```shell
git checkout upstream-master
git pull
git subtree split --prefix=crates/egui-winit --onto upstream-egui-winit -b upstream-egui-winit
```