module.exports = {
  '**/*.{js,ts,html,md,json}':
    'prettier --write --ignore-path ../../.prettierignore',
  '*.ts': 'tslint --fix',
  'projects/ui/**/*.ts': () => 'npm run check:ui',
  'projects/shared/**/*.ts': () => 'npm run check:shared',
  'projects/marketplace/**/*.ts': () => 'npm run check:marketplace',
  'projects/setup-wizard/**/*.ts': () => 'npm run check:setup',
  'projects/start-tunnel/**/*.ts': () => 'npm run check:tunnel',
  'projects/**/*.{ts,html}': () => 'npm run check:i18n',
}
