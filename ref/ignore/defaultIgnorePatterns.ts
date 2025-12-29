/**
 * 统一的默认忽略规则常量
 * 合并了原有的 defaultIgnorePatterns.ts 和 filesystem/defaultIgnorePatterns.ts
 * 作为项目中唯一的默认忽略规则源
 * 
 * 规则分类：
 * - 版本控制
 * - 依赖目录
 * - 日志文件
 * - 运行时数据
 * - 测试覆盖率
 * - 构建工具缓存
 * - 编译输出
 * - 操作系统生成文件
 * - 编辑器文件
 * - 临时文件
 * - 语言特定文件
 */

export const DEFAULT_IGNORE_PATTERNS: string[] = [
  // Version control
  '.git/**',
  '.hg/**',
  '.hgignore',
  '.svn/**',

  // Dependency directories
  '**/node_modules/**',
  '**/bower_components/**',
  '**/jspm_packages/**',
  'vendor/**',
  '**/.bundle/**',
  '**/.gradle/**',
  'target/**',

  // Logs
  'logs/**',
  '**/*.log',
  '**/npm-debug.log*',
  '**/yarn-debug.log*',
  '**/yarn-error.log*',

  // Runtime data
  'pids/**',
  '*.pid',
  '*.seed',
  '*.pid.lock',

  // Coverage directory used by tools like istanbul
  'coverage/**',
  '.nyc_output/**',

  // Grunt intermediate storage
  '.grunt/**',

  // Compiled binary addons
  'build/Release/**',

  // TypeScript v1 declaration files
  'typings/**',

  // Optional npm cache directory
  '**/.npm/**',

  // Cache directories
  '.eslintcache',
  '.rollup.cache/**',
  '.webpack.cache/**',
  '.parcel-cache/**',
  '.sass-cache/**',
  '*.cache',

  // Optional REPL history
  '.node_repl_history',

  // Output of 'npm pack'
  '*.tgz',

  // Yarn files
  '**/.yarn/**',
  '**/.yarn-integrity',

  // dotenv environment variables file
  '.env',

  // next.js build output
  '.next/**',

  // nuxt.js build output
  '.nuxt/**',

  // vuepress build output
  '.vuepress/dist/**',

  // Serverless directories
  '.serverless/**',

  // FuseBox cache
  '.fusebox/**',

  // DynamoDB Local files
  '.dynamodb/**',

  // TypeScript output
  'dist/**',

  // OS generated files
  '**/.DS_Store',
  '**/Thumbs.db',

  // Editor directories and files
  '.idea/**',
  '.vscode/**',
  '**/*.swp',
  '**/*.swo',
  '**/*.swn',
  '**/*.bak',
  '**/.#*',     // Emacs风格的锁文件
  '**/#*#',     // Emacs风格的备份文件
  '**/.*.sw[pon]', // Vim交换文件变体
  '**/.*.un~',  // Emacs备份文件变体
  '**/.*.bak',  // 隐藏的备份文件

  // Build outputs
  'build/**',
  'out/**',

  // Temporary files
  'tmp/**',
  'temp/**',

  // repomix output
  '**/repomix-output.*',
  '**/repopack-output.*', // Legacy

  // Essential Node.js-related entries
  '**/package-lock.json',
  '**/yarn-error.log',
  '**/yarn.lock',
  '**/pnpm-lock.yaml',
  '**/bun.lockb',
  '**/bun.lock',

  // Essential Python-related entries
  '**/__pycache__/**',
  '**/*.py[cod]',
  '**/venv/**',
  '**/.venv/**',
  '**/.pytest_cache/**',
  '**/.mypy_cache/**',
  '**/.ipynb_checkpoints/**',
  '**/Pipfile.lock',
  '**/poetry.lock',
  '**/uv.lock',

  // Essential Rust-related entries
  '**/Cargo.lock',
  '**/Cargo.toml.orig',
  '**/target/**',
  '**/*.rs.bk',

  // Essential PHP-related entries
  '**/composer.lock',

  // Essential Ruby-related entries
  '**/Gemfile.lock',

  // Essential Go-related entries
  '**/go.sum',

  // Essential Elixir-related entries
  '**/mix.lock',

  // Essential Haskell-related entries
  '**/stack.yaml.lock',
  '**/cabal.project.freeze'
];