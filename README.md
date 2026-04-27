# mymc-gtk

**mymc-gtk** is a modern, graphical frontend for the classic PlayStation 2 memory card manager `mymc`. 

Built from the ground up using **Rust** and **GTK4**, this project was created to provide a seamless, native experience for modern Linux desktops running **Wayland** in 2026. It resolves legacy compatibility issues while offering a robust and visually native UI that preserves the core functionality of the original tool.

Under the hood, it uses PyO3 to interact with the Python 3 core API in `python_core/api.py`. The app can open `.ps2` images, list saves, import/export `.psu`, and delete saves effortlessly.

## Credits & Upstream

All credit for the core memory card manipulation logic goes to the official [ps2dev/mymc](https://github.com/ps2dev/mymc) project (originally authored by Ross Ridge). This repository solely provides a modernized GTK4 wrapper around that brilliant foundation.

## Build Requirements (Fedora)

Install system packages:

```bash
sudo dnf install -y gtk4-devel python3-devel
```

## Build & Run

```bash
cargo build
cargo run
```
