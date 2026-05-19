pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

blueprints = custom_target('blueprints',
  input: files(
    'shortcuts-dialog.blp',
    'window.blp',
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

gnome.compile_resources('folios',
  'folios.gresource.xml',
  gresource_bundle: true,
  dependencies: blueprints,
  install: true,
  install_dir: pkgdatadir,
)

conf = configuration_data()
conf.set_quoted('VERSION', meson.project_version())
conf.set_quoted('GETTEXT_PACKAGE', 'folios')
conf.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set_quoted('PKGDATADIR', pkgdatadir)

configure_file(
    input: 'config.rs.in',
    output: 'config.rs',
    configuration: conf
)

# Copy the config.rs output to the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true
)

cargo_bin = find_program('cargo')
sh = find_program('sh')
cargo_home = get_option('cargo-home')
cargo_target_dir = meson.project_build_root() / 'src'

cargo_env = environment()
cargo_env.set('CARGO_TARGET_DIR', cargo_target_dir)

if cargo_home != ''
  cargo_env.set('CARGO_HOME', cargo_home)
else
  cargo_env.set('CARGO_HOME', meson.project_build_root() / 'cargo-home')
endif

if get_option('buildtype') == 'release'
  cargo_build_type = '--release'
  rust_target = 'release'
else
  cargo_build_type = ''
  rust_target = 'debug'
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
    'cargo build $1 --manifest-path "$2" && cp "$3/$4/folios" "$5"',
    'sh',
    cargo_build_type,
    meson.project_source_root() / 'Cargo.toml',
    cargo_target_dir,
    rust_target,
    '@OUTPUT@',
  ],
  env: cargo_env,
)
