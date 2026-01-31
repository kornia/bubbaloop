# Changelog

## [0.1.0](https://github.com/kornia/bubbaloop/compare/v0.0.2...v0.1.0) (2026-01-31)


### Features

* add bubbaloop CLI with plugin scaffolding ([e6d0a3e](https://github.com/kornia/bubbaloop/commit/e6d0a3eace4987416a6b504483bb70c4f19ec7c6))
* add openmeteo weather publisher and dashboard JSON panel ([#11](https://github.com/kornia/bubbaloop/issues/11)) ([c829670](https://github.com/kornia/bubbaloop/commit/c829670ac475e594da87195dd2de8f7c4328f338))
* **ci:** add release-please automation for versioning and releases ([#14](https://github.com/kornia/bubbaloop/issues/14)) ([bf73254](https://github.com/kornia/bubbaloop/commit/bf73254b7523b7adacb2c102aa39cd71d19088c9))
* comprehensive installer with Zenoh and systemd setup ([49d43e6](https://github.com/kornia/bubbaloop/commit/49d43e6db71f196cd49fab51dd30996b58aa0a3f))
* **dashboard:** add JSON panel for viewing raw data ([#10](https://github.com/kornia/bubbaloop/issues/10)) ([20dd3d2](https://github.com/kornia/bubbaloop/commit/20dd3d26db96d4b1fe94cb8e6ba93ea353738fe9))
* **dashboard:** refactor stats panel to show all network topics ([6916255](https://github.com/kornia/bubbaloop/commit/6916255b17368cbcb0602473a317f2db9a0038ee))
* merge daemon into CLI, single 7.4MB binary + ratatui TUI ([#20](https://github.com/kornia/bubbaloop/issues/20)) ([268f8ff](https://github.com/kornia/bubbaloop/commit/268f8ff0c00c7969880024439b8cdf137abb73da))
* publish TUI to npm on release ([276b5d9](https://github.com/kornia/bubbaloop/commit/276b5d9dd59fe0d31fad6b062679f2d54712bf0b))
* **tui:** add ability to link external plugins ([aa39472](https://github.com/kornia/bubbaloop/commit/aa39472dcdfa1f83faf6ec405e17756bfaa06614))
* **tui:** add plugin management view ([497010d](https://github.com/kornia/bubbaloop/commit/497010d5ad6a7847f54fb6a634fca309cf7da588))
* **tui:** implement direct plugin creation without CLI ([3acd189](https://github.com/kornia/bubbaloop/commit/3acd18905d4d251cd59639ad8eb87ab5a38b88ec))
* **tui:** redesign plugin system with registry and systemd ([56cb0c8](https://github.com/kornia/bubbaloop/commit/56cb0c8d38ca67f2287f2b82cc4381fc7e9b8997))


### Bug Fixes

* add --experimental-wasm-modules flag for zenoh-ts WASM support ([259be8c](https://github.com/kornia/bubbaloop/commit/259be8c67fa1a278c61edbd3e035e4919352ebcf))
* add wrapper script to enable WASM modules automatically ([e170b43](https://github.com/kornia/bubbaloop/commit/e170b432ed1ac597393c4fad87b71ee1b68f85f4))
* **dashboard:** match listener status across topic formats ([ea8e476](https://github.com/kornia/bubbaloop/commit/ea8e476a96f843a154124b777b63d22377fd98f9))
* **tui:** resolve duplicate header and screen corruption during view transitions ([#13](https://github.com/kornia/bubbaloop/issues/13)) ([a40cd32](https://github.com/kornia/bubbaloop/commit/a40cd320414ad738dc8d227270a3fc5e68b927ac))
* **tui:** use cargo run and venv python in service units ([3fd7b0a](https://github.com/kornia/bubbaloop/commit/3fd7b0adfd8efc1c84e803bacec3e604339ae6a7))

## [0.0.2](https://github.com/kornia/bubbaloop/compare/v0.0.1...v0.0.2) (2026-01-25)


### Bug Fixes

* add --experimental-wasm-modules flag for zenoh-ts WASM support ([259be8c](https://github.com/kornia/bubbaloop/commit/259be8c67fa1a278c61edbd3e035e4919352ebcf))
