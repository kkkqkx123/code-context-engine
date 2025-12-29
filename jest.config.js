// 条件性配置：只有在运行测试时才使用mock
const isTest = process.env.NODE_ENV === 'test';

module.exports = {
  testEnvironment: 'node',
  roots: ['<rootDir>/src'],
  testMatch: [
    '**/?(*.)+(test).ts',
    '**/__tests__/**/*.test.ts'
  ],
  transform: {
    '^.+\\.tsx?$': ['ts-jest', {
      tsconfig: 'tsconfig.json'
    }],
    '^.+\\.jsx?$': 'babel-jest', // 添加对JS文件的转换支持
  },
  transformIgnorePatterns: [
    'node_modules/(?!(uuid)/)' // 允许uuid模块被转换
  ],
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.d.ts',
    '!src/**/*.test.ts',
    '!src/**/__tests__/**',
  ],
  coverageDirectory: 'coverage',
  coverageReporters: ['text', 'lcov', 'html'],
  moduleNameMapper: {
    ...(isTest ? {
      '^@nebula-contrib/nebula-nodejs$': '<rootDir>/__mocks__/@nebula-contrib/nebula-nodejs.js'
    } : {}),
    '^@test/(.*)$': '<rootDir>/test/$1',
    '^src/(.*)$': '<rootDir>/src/$1'
  },
  testTimeout: 10000,
  verbose: true,
};