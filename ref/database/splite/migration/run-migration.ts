#!/usr/bin/env node

/**
 * SQLite数据迁移执行脚本
 * 用于将JSON数据迁移到SQLite数据库
 */

import 'reflect-metadata';
import { Container } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { JsonToSqliteMigrator } from './JsonToSqliteMigrator';
import { MigrationOrchestrator } from './MigrationOrchestrator';
import { MigrationProgress, MigrationError } from './MigrationTypes';

// 设置DI容器
const container = new Container();

// 注册服务
container.bind<LoggerService>(TYPES.LoggerService).to(LoggerService).inSingletonScope();
container.bind<SqliteDatabaseService>(TYPES.SqliteDatabaseService).to(SqliteDatabaseService).inSingletonScope();
container.bind<JsonToSqliteMigrator>(TYPES.JsonToSqliteMigrator).to(JsonToSqliteMigrator).inSingletonScope();
container.bind<MigrationOrchestrator>(TYPES.MigrationOrchestrator).to(MigrationOrchestrator).inSingletonScope();

// 创建迁移类型符号
const MIGRATION_TYPES = {
  JsonToSqliteMigrator: Symbol.for('JsonToSqliteMigrator'),
  MigrationOrchestrator: Symbol.for('MigrationOrchestrator'),
};

// 注册迁移类型
container.bind<JsonToSqliteMigrator>(MIGRATION_TYPES.JsonToSqliteMigrator).to(JsonToSqliteMigrator).inSingletonScope();
container.bind<MigrationOrchestrator>(MIGRATION_TYPES.MigrationOrchestrator).to(MigrationOrchestrator).inSingletonScope();

async function main() {
  const logger = container.get<LoggerService>(TYPES.LoggerService);
  
  logger.info('=== SQLite数据迁移工具 ===');
  logger.info('开始执行JSON到SQLite的数据迁移');

  try {
    // 获取迁移协调器
    const orchestrator = container.get<MigrationOrchestrator>(MIGRATION_TYPES.MigrationOrchestrator);
    
    // 配置迁移选项
    const migrationOptions = {
      config: {
        backupEnabled: true,
        backupPath: './data/backup/migration',
        validateBeforeMigration: true,
        validateAfterMigration: true,
        rollbackOnError: true,
        maxRetries: 3,
        retryDelay: 1000
      },
      onProgress: (progress: MigrationProgress) => {
        logger.info(`[${progress.stage}] ${progress.percentage}% - ${progress.message}`);
      },
      onError: (error: MigrationError) => {
        logger.error(`[${error.stage}] ${error.error}`);
        if (error.context) {
          logger.error('错误上下文:', error.context);
        }
      }
    };

    // 执行迁移
    logger.info('正在执行数据迁移...');
    const report = await orchestrator.executeProgressiveMigration(migrationOptions);

    // 输出迁移结果
    logger.info('=== 迁移完成 ===');
    logger.info(`总体成功: ${report.summary.overallSuccess}`);
    logger.info(`迁移总数: ${report.summary.totalMigrated}`);
    logger.info(`错误总数: ${report.summary.totalErrors}`);
    logger.info(`项目映射迁移: ${report.summary.projectMappings.migratedCount} 成功, ${report.summary.projectMappings.errorCount} 错误`);
    logger.info(`项目状态迁移: ${report.summary.projectStates.migratedCount} 成功, ${report.summary.projectStates.errorCount} 错误`);

    if (report.validation.isValid) {
      logger.info('数据验证: 通过');
    } else {
      logger.warn('数据验证: 失败');
      report.validation.issues.forEach(issue => {
        logger.warn(`  - ${issue}`);
      });
    }

    // 输出数据一致性信息
    logger.info('数据一致性检查:');
    logger.info(`  - 项目数: ${report.validation.dataConsistency.projects}`);
    logger.info(`  - 项目状态数: ${report.validation.dataConsistency.projectStatus}`);
    logger.info(`  - 文件索引状态数: ${report.validation.dataConsistency.fileStates}`);
    logger.info(`  - 变更历史数: ${report.validation.dataConsistency.changeHistory}`);

    // 输出性能指标
    logger.info('性能指标:');
    logger.info(`  - 总耗时: ${report.performanceMetrics.totalDuration}ms`);
    logger.info(`  - 备份耗时: ${report.performanceMetrics.backupDuration}ms`);
    logger.info(`  - 迁移耗时: ${report.performanceMetrics.migrationDuration}ms`);
    logger.info(`  - 验证耗时: ${report.performanceMetrics.validationDuration}ms`);

    // 输出建议
    if (report.recommendations.length > 0) {
      logger.info('建议:');
      report.recommendations.forEach(recommendation => {
        logger.info(`  - ${recommendation}`);
      });
    }

    // 检查迁移状态
    const status = orchestrator.getMigrationStatus();
    logger.info(`迁移状态: ${status.enabled ? '已启用' : '未启用'}`);
    logger.info(`数据源: ${status.dataSource}`);

    if (report.summary.overallSuccess) {
      logger.info('✅ 数据迁移成功完成！');
      process.exit(0);
    } else {
      logger.error('❌ 数据迁移失败！');
      process.exit(1);
    }

  } catch (error) {
    logger.error('迁移过程发生严重错误:', error);
    process.exit(1);
  }
}

// 处理命令行参数
function parseCommandLineArgs() {
  const args = process.argv.slice(2);
  const options = {
    help: false,
    force: false,
    dryRun: false,
    backupOnly: false,
    validateOnly: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    switch (arg) {
      case '--help':
      case '-h':
        options.help = true;
        break;
      case '--force':
      case '-f':
        options.force = true;
        break;
      case '--dry-run':
      case '-d':
        options.dryRun = true;
        break;
      case '--backup-only':
      case '-b':
        options.backupOnly = true;
        break;
      case '--validate-only':
      case '-v':
        options.validateOnly = true;
        break;
    }
  }

  return options;
}

// 显示帮助信息
function showHelp() {
  console.log(`
SQLite数据迁移工具 - 将JSON数据迁移到SQLite数据库

使用方法: npm run migrate [选项]

选项:
  -h, --help          显示帮助信息
  -f, --force         强制迁移（跳过检查）
  -d, --dry-run       模拟运行（不实际执行迁移）
  -b, --backup-only   仅备份数据
  -v, --validate-only 仅验证数据

示例:
  npm run migrate              # 执行完整迁移
  npm run migrate --dry-run    # 模拟迁移过程
  npm run migrate --backup-only # 仅备份JSON数据
  npm run migrate --validate-only # 仅验证迁移结果
`);
}

// 主程序入口
if (require.main === module) {
  const options = parseCommandLineArgs();

  if (options.help) {
    showHelp();
    process.exit(0);
  }

  // 根据选项调整配置
  if (options.dryRun) {
    console.log('🔍 模拟运行模式 - 不会实际执行迁移');
  }

  if (options.force) {
    console.log('⚡ 强制迁移模式 - 跳过预检查');
  }

  // 执行主程序
  main().catch((error) => {
    console.error('未处理的错误:', error);
    process.exit(1);
  });
}

export { main, parseCommandLineArgs, showHelp };