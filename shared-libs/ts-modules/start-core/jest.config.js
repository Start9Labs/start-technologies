/** @type {import('ts-jest').JestConfigWithTsJest} */
module.exports = {
  preset: 'ts-jest',
  automock: false,
  transform: {
    '^.+\\.[tj]s$': ['ts-jest', { tsconfig: { allowJs: true } }],
  },
  transformIgnorePatterns: ['node_modules/(?!mime/)'],
  testEnvironment: 'node',
  rootDir: './lib/',
  modulePathIgnorePatterns: ['./dist/'],
}
