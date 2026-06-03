application_id = 'com.fvtronics.Quire'

full_color_dir = 'hicolor' / 'scalable' / 'apps'
install_data(
  full_color_dir / ('@0@.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / full_color_dir
)

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir
)
