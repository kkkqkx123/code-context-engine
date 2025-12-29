import { injectable, inject } from 'inversify';
import { TYPES } from '../types';
import { ProjectIdManager } from './ProjectIdManager';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { HybridIndexService } from '../service/index/HybridIndexService';

@injectable()
export class ProjectLookupService {
  private projectIdManager: ProjectIdManager;
  private errorHandler: ErrorHandlerService;
  public indexService: HybridIndexService; // Public for access from ProjectRoutes

  constructor(
    @inject(TYPES.ProjectIdManager) projectIdManager: ProjectIdManager,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.HybridIndexService) indexService: HybridIndexService
  ) {
    this.projectIdManager = projectIdManager;
    this.errorHandler = errorHandler;
    this.indexService = indexService;
  }

  async getProjectIdByCollection(collectionName: string): Promise<string | null> {
    try {
      // Parse project ID from collection name
      if (collectionName.startsWith('project-')) {
        return collectionName.substring(8); // Remove 'project-' prefix
      }
      return null;
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectIdByCollection'
      });
      return null;
    }
  }

  async getProjectIdBySpace(spaceName: string): Promise<string | null> {
    try {
      // Parse project ID from space name
      if (spaceName.startsWith('project_')) {
        return spaceName.substring(8); // Remove 'project_' prefix
      }
      return null;
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectIdBySpace'
      });
      return null;
    }
  }

  async getProjectPathByProjectId(projectId: string): Promise<string | null> {
    try {
      // Get project path from project ID using the project ID manager
      const projectPath = this.projectIdManager.getProjectPath(projectId);
      return projectPath || null;
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectPathByProjectId'
      });
      return null;
    }
  }

  async getProjectPathByCollection(collectionName: string): Promise<string | null> {
    try {
      const projectId = await this.getProjectIdByCollection(collectionName);
      if (!projectId) {
        return null;
      }
      return this.getProjectPathByProjectId(projectId);
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectPathByCollection'
      });
      return null;
    }
  }

  async getProjectPathBySpace(spaceName: string): Promise<string | null> {
    try {
      const projectId = await this.getProjectIdBySpace(spaceName);
      if (!projectId) {
        return null;
      }
      return this.getProjectPathByProjectId(projectId);
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectPathBySpace'
      });
      return null;
    }
  }

  // Get the project ID with the latest update time
  async getLatestUpdatedProjectId(): Promise<string | null> {
    try {
      return this.projectIdManager.getLatestUpdatedProject();
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getLatestUpdatedProjectId'
      });
      return null;
    }
  }

  // Get project path for the latest updated project
  async getProjectPathForLatestUpdatedProject(): Promise<string | null> {
    try {
      const latestProjectId = await this.getLatestUpdatedProjectId();
      if (!latestProjectId) {
        return null;
      }
      return this.getProjectPathByProjectId(latestProjectId);
    } catch (error) {
      this.errorHandler.handleError(error as Error, {
        component: 'ProjectLookupService',
        operation: 'getProjectPathForLatestUpdatedProject'
      });
      return null;
    }
  }
}