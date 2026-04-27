# mymc-gtk

Native GTK4 frontend for `mymc`, using PyO3 to call the Python 3 core API in `python_core/api.py`.

## Build Requirements (Fedora)

Install system packages:

```bash
sudo dnf install -y gtk4-devel python3-devel
```

## Build

```bash
cargo build
```

## Run

```bash
cargo run
```

The app can open `.ps2` images, list saves, import/export `.psu`, and delete saves.

## Credits

This project is a modern GTK4/Rust frontend wrapper based on the original [mymc](http://www.csclub.uwaterloo.ca:11068/mymc/) utility written in Python by Ross Ridge.
