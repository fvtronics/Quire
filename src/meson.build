pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

blueprints = custom_target('blueprints',
  input: files(
    'shortcuts-dialog.blp',
    'window.blp',
    'merge-workspace.blp',
    'compress-workspace.blp',
    'metadata-workspace.blp',
    'organize-workspace.blp',
    'extract-workspace.blp',
    'split-workspace.blp',
    'watermark-workspace.blp',
    'password-dialog.blp',
  ),
  output: '.',
  command: [
    find_program('blueprint-compiler'),
    'batch-compile',
    '--minify',
    '@OUTPUT@',
    '@CURRENT_SOURCE_DIR@',
    '@INPUT@',
  ],
)

gnome.compile_resources('quire',
  'quire.gresource.xml',
  gresource_bundle: true,
  dependencies: blueprints,
  install: true,
  install_dir: pkgdatadir,
)

cargo_bin = find_program('cargo')
sh = find_program('sh')
cargo_home = get_option('cargo-home')
cargo_target_dir = meson.project_build_root() / 'src'

cargo_env = environment()
cargo_env.set('CARGO_TARGET_DIR', cargo_target_dir)
cargo_env.set('QUIRE_VERSION', meson.project_version())
cargo_env.set('QUIRE_GETTEXT_PACKAGE', 'quire')
cargo_env.set('QUIRE_LOCALEDIR', get_option('prefix') / get_option('localedir'))
cargo_env.set('QUIRE_PKGDATADIR', pkgdatadir)

if cargo_home != ''
  cargo_env.set('CARGO_HOME', cargo_home)
else
  cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo-home')
endif

cargo_build = custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    sh,
    '-c',
    'cargo build --release --manifest-path "$1" && cp "$2/release/quire" "$3"',
    'sh',
    meson.project_source_root() / 'Cargo.toml',
    cargo_target_dir,
    '@OUTPUT@',
  ],
  env: cargo_env,
)
