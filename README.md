# Folios

Folios is a simple GNOME utility for working with local PDF files without a
complex document suite.

It can merge multiple PDFs, organize pages, extract page ranges, split
documents, and compress files without sending them to an online service.

## Screenshots

Merge PDFs | Organize pages | Extract pages | Split documents | Compress files
:------------------:|:-----------------:|:----------------:|:---------------------------:|:----------------:
![Merge PDFs](data/screenshots/merge.png?raw=true "Merge multiple PDF files") | ![Organize pages](data/screenshots/organize.png?raw=true "Organize PDF pages") | ![Extract pages](data/screenshots/extract.png?raw=true "Extract PDF pages") | ![Split documents](data/screenshots/split.png?raw=true "Split PDF documents") | ![Compress files](data/screenshots/compress.png?raw=true "Compress PDF files")

## How to install

### Flatpak

Folios can be built and installed locally with Flatpak Builder:

```sh
flatpak-builder --user --install --install-deps-from=flathub build-dir com.fvtronics.folios.json --force-clean
```

### Build from source

Folios uses the [meson build system](http://mesonbuild.com/). Run the following
commands to clone Folios and initialize the build:

```sh
git clone https://gitlab.com/fvtronics/folios.git
cd folios
meson setup build
```

To install the built package on your system, run the following command:

```sh
meson install -C build
```

## License

Licensed under the GPLv3. See the
[COPYING](COPYING) file for the
full license information.
