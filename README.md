# Quire

Quire is a simple GNOME utility for working with local PDF files without a
complex document suite.

It can merge multiple PDFs, organize pages, extract page ranges, split
documents, compress files, and edit metadata without sending them to an online
service.

[![Scanned on SonarQube](https://sonarcloud.io/images/project_badges/sonarcloud-white.svg)](https://sonarcloud.io/summary/overall?id=fvtronics_quire)

[![Quality Gate Status](https://sonarcloud.io/api/project_badges/measure?project=fvtronics_quire&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=fvtronics_quire)
[![Maintainability Rating](https://sonarcloud.io/api/project_badges/measure?project=fvtronics_quire&metric=sqale_rating)](https://sonarcloud.io/summary/overall?id=fvtronics_quire)
[![Security Rating](https://sonarcloud.io/api/project_badges/measure?project=fvtronics_quire&metric=security_rating)](https://sonarcloud.io/summary/overall?id=fvtronics_quire)
[![Reliability Rating](https://sonarcloud.io/api/project_badges/measure?project=fvtronics_quire&metric=reliability_rating)](https://sonarcloud.io/summary/overall?id=fvtronics_quire)

## Screenshots

Merge PDFs | Organize pages | Extract pages | Split documents | Compress files | Edit metadata
:------------------:|:-----------------:|:----------------:|:---------------------------:|:----------------:|:----------------:
![Merge PDFs](data/screenshots/merge.png?raw=true "Merge multiple PDF files") | ![Organize pages](data/screenshots/organize.png?raw=true "Organize PDF pages") | ![Extract pages](data/screenshots/extract.png?raw=true "Extract PDF pages") | ![Split documents](data/screenshots/split.png?raw=true "Split PDF documents") | ![Compress files](data/screenshots/compress.png?raw=true "Compress PDF files") | ![Edit metadata](data/screenshots/metadata.png?raw=true "Edit PDF metadata")

## How to install

### AUR&nbsp;&nbsp;[![AUR package](https://repology.org/badge/version-for-repo/aur/quire.svg?header=)](https://repology.org/project/quire/versions)

Arch based distributions can install Quire from the [AUR](https://aur.archlinux.org/packages/quire), or using an aurhelper such as yay: `yay -S quire`

### Flatpak

Quire can be built and installed locally with Flatpak Builder:

```sh
flatpak-builder --user --install --install-deps-from=flathub build-dir com.fvtronics.Quire.json --force-clean
```

### Build from source

Quire uses the [meson build system](http://mesonbuild.com/). Run the following
commands to clone Quire and initialize the build:

```sh
git clone https://codeberg.org/fvtronics/quire.git
cd quire
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
