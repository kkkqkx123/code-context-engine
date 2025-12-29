import { injectable } from 'inversify';

export interface ISpaceNameUtils {
  generateSpaceName(projectId: string): string;
  validateSpaceName(spaceName: string): boolean;
  generateSpaceNameFromPath(projectPath: string): string;
  sanitizeName(name: string): string;
}

@injectable()
export class SpaceNameUtils implements ISpaceNameUtils {
  
  /**
   * 生成空间名称
   * @param projectId 项目ID
   * @returns 格式化的空间名称
   */
  generateSpaceName(projectId: string): string {
    if (!projectId) {
      throw new Error('Project ID is required');
    }
    
    // 确保projectId不包含特殊字符，只保留字母、数字和下划线
    const sanitizedProjectId = this.sanitizeName(projectId);
    return `project_${sanitizedProjectId}`;
  }

  /**
   * 验证空间名称是否有效
   * @param spaceName 空间名称
   * @returns 是否有效
   */
  validateSpaceName(spaceName: string): boolean {
    if (!spaceName || typeof spaceName !== 'string') {
      return false;
    }

    // 检查是否是无效的字符串
    if (spaceName === 'undefined' || spaceName === '') {
      return false;
    }

    // Nebula Graph 对空间名称有特定要求：
    // - 必须以字母开头
    // - 只能包含字母、数字和下划线
    // - 长度在1到64个字符之间
    const namePattern = /^[a-zA-Z][a-zA-Z0-9_]*$/;
    return namePattern.test(spaceName) && spaceName.length <= 64;
  }

  /**
   * 从项目路径生成空间名称
   * @param projectPath 项目路径
   * @returns 格式化的空间名称
   */
  generateSpaceNameFromPath(projectPath: string): string {
    if (!projectPath) {
      throw new Error('Project path is required');
    }

    // 处理项目路径，将其转换为有效的空间名称
    // 1. 移除路径分隔符和特殊字符
    // 2. 确保只包含字母、数字和下划线
    // 3. 限制长度
    let sanitizedName = projectPath
      .replace(/[^a-zA-Z0-9_]/g, '_')  // 替换所有非字母数字下划线字符为下划线
      .replace(/_+/g, '_')             // 将多个连续下划线替换为单个下划线
      .replace(/^_|_$/g, '');          // 移除开始和结尾的下划线

    // 如果处理后的名称为空或无效，使用默认名称
    if (!sanitizedName || !/^[a-zA-Z]/.test(sanitizedName)) {
      // 如果不以字母开头，添加前缀
      sanitizedName = `project_${sanitizedName || 'default'}`;
    }

    // 限制长度
    if (sanitizedName.length > 50) {
      sanitizedName = sanitizedName.substring(0, 50);
    }

    return sanitizedName.toLowerCase(); // 转换为小写以确保一致性
  }

  /**
   * 清理名称，确保符合Nebula Graph命名规范
   * @param name 原始名称
   * @returns 清理后的名称
   */
  sanitizeName(name: string): string {
    if (!name) {
      return '';
    }

    // 替换所有非字母数字下划线字符为下划线
    let sanitizedName = name.replace(/[^a-zA-Z0-9_]/g, '_');
    
    // 确保以字母开头
    if (sanitizedName && !/^[a-zA-Z]/.test(sanitizedName)) {
      sanitizedName = `item_${sanitizedName}`;
    }
    
    // 限制最大长度
    if (sanitizedName.length > 64) {
      sanitizedName = sanitizedName.substring(0, 64);
    }
    
    return sanitizedName;
  }
}