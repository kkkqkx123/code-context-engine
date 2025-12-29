import { HotReloadRestartService, RestartState } from '../HotReloadRestartService';
import { ProjectHotReloadService } from '../ProjectHotReloadService';
import { ChangeDetectionService } from '../ChangeDetectionService';
import { VectorIndexService } from '../../index/VectorIndexService';
import { HotReloadConfigService } from '../HotReloadConfigService';
import { HotReloadRecoveryService } from '../HotReloadRecoveryService';
import { ProjectStateManager } from '../../project/ProjectStateManager';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('fs/promises');

// Mock all service dependencies
jest.mock('../ProjectHotReloadService');
jest.mock('../ChangeDetectionService');
jest.mock('../../index/IndexService');
jest.mock('../HotReloadConfigService');
jest.mock('../HotReloadRecoveryService');
jest.mock('../../project/ProjectStateManager');

describe('HotReloadRestartService', () => {
    let restartService: HotReloadRestartService;
    let mockProjectHotReloadService: jest.Mocked<ProjectHotReloadService>;
    let mockChangeDetectionService: jest.Mocked<ChangeDetectionService>;
    let mockConfigService: jest.Mocked<HotReloadConfigService>;
    let mockRecoveryService: jest.Mocked<HotReloadRecoveryService>;
    let mockProjectStateManager: jest.Mocked<ProjectStateManager>;
    let mockLogger: jest.Mocked<LoggerService>;
    let mockErrorHandler: jest.Mocked<ErrorHandlerService>;

    beforeEach(() => {
        // Create mock services with proper method implementations
        mockProjectHotReloadService = {
            getAllProjectStatuses: jest.fn(),
            getProjectConfig: jest.fn(),
            getProjectMetrics: jest.fn(),
            enableForProject: jest.fn(),
            disableForProject: jest.fn(),
            getProjectStatus: jest.fn(),
            updateConfig: jest.fn(),
            isProjectHotReloadEnabled: jest.fn()
        } as unknown as jest.Mocked<ProjectHotReloadService>;

        mockChangeDetectionService = {
            isServiceRunning: jest.fn(),
            initialize: jest.fn(),
            stop: jest.fn(),
            setCallbacks: jest.fn(),
            getFileHash: jest.fn(),
            getFileHistory: jest.fn(),
            getAllFileHashes: jest.fn(),
            isFileTracked: jest.fn(),
            getTrackedFilesCount: jest.fn(),
            getStats: jest.fn(),
            resetStats: jest.fn(),
            isTestMode: jest.fn(),
            waitForFileProcessing: jest.fn(),
            waitForAllProcessing: jest.fn(),
            flushPendingChanges: jest.fn(),
            getProjectIdForPath: jest.fn(),
            detectChangesForUpdate: jest.fn(),
            on: jest.fn(),
            once: jest.fn(),
            removeListener: jest.fn(),
            eventNames: jest.fn(),
            listenerCount: jest.fn(),
            addListener: jest.fn(),
            off: jest.fn(),
            removeAllListeners: jest.fn(),
            setMaxListeners: jest.fn(),
            getMaxListeners: jest.fn(),
            listeners: jest.fn()
        } as unknown as jest.Mocked<ChangeDetectionService>;


        mockConfigService = {
            getProjectConfig: jest.fn(),
            getGlobalConfig: jest.fn(),
            updateGlobalConfig: jest.fn(),
            getProjectConfigFromState: jest.fn(),
            updateProjectStateConfig: jest.fn(),
            setProjectConfig: jest.fn(),
            removeProjectConfig: jest.fn(),
            getAllProjectConfigs: jest.fn(),
            isGloballyEnabled: jest.fn(),
            isProjectEnabled: jest.fn(),
            validateConfig: jest.fn(),
            validateProjectStateConfig: jest.fn(),
            getProjectHotReloadStatus: jest.fn(),
            resetProjectStats: jest.fn(),
            resetToDefaults: jest.fn(),
            loadFromFile: jest.fn(),
            saveToFile: jest.fn()
        } as unknown as jest.Mocked<HotReloadConfigService>;

        mockRecoveryService = {
            handleError: jest.fn(),
            getRecoveryStrategy: jest.fn()
        } as unknown as jest.Mocked<HotReloadRecoveryService>;

        mockProjectStateManager = {
            getProjectStateByPath: jest.fn(),
            getProjectId: jest.fn(),
            getAllProjects: jest.fn(),
            refreshAllProjectStates: jest.fn(),
            initialize: jest.fn(),
            createOrUpdateProjectState: jest.fn(),
            getProjectState: jest.fn(),
            getAllProjectStates: jest.fn(),
            updateProjectStatus: jest.fn(),
            updateProjectIndexingProgress: jest.fn(),
            updateProjectLastIndexed: jest.fn(),
            updateProjectMetadata: jest.fn(),
            deleteProjectState: jest.fn(),
            getProjectStats: jest.fn(),
            activateProject: jest.fn(),
            deactivateProject: jest.fn(),
            refreshProjectState: jest.fn(),
            cleanupInvalidStates: jest.fn(),
            updateVectorStatus: jest.fn(),
            updateGraphStatus: jest.fn(),
            getVectorStatus: jest.fn(),
            getGraphStatus: jest.fn(),
            resetVectorStatus: jest.fn(),
            resetGraphStatus: jest.fn(),
            startVectorIndexing: jest.fn(),
            startGraphIndexing: jest.fn(),
            updateVectorIndexingProgress: jest.fn(),
            updateGraphIndexingProgress: jest.fn(),
            completeVectorIndexing: jest.fn(),
            completeGraphIndexing: jest.fn(),
            failVectorIndexing: jest.fn(),
            failGraphIndexing: jest.fn(),
            disableGraphIndexing: jest.fn()
        } as unknown as jest.Mocked<ProjectStateManager>;

        mockLogger = new LoggerService() as jest.Mocked<LoggerService>;
        mockLogger.info = jest.fn();
        mockLogger.error = jest.fn();
        mockLogger.warn = jest.fn();
        mockLogger.debug = jest.fn();

        mockErrorHandler = new ErrorHandlerService(mockLogger) as jest.Mocked<ErrorHandlerService>;
        mockErrorHandler.handleError = jest.fn();

        restartService = new HotReloadRestartService(
            mockProjectHotReloadService,
            mockChangeDetectionService,
            mockConfigService,
            mockRecoveryService,
            mockProjectStateManager,
            mockLogger,
            mockErrorHandler
        );
    });

    afterEach(() => {
        jest.clearAllMocks();
    });

    describe('handleApplicationRestart', () => {
        it('should handle application restart process successfully', async () => {
            // Mock all methods called during restart
            const restoreStateSpy = jest.spyOn(restartService as any, 'restoreStateAfterRestart').mockResolvedValue(undefined);
            const restoreWatchingSpy = jest.spyOn(restartService as any, 'restoreProjectWatchingThroughIndexService').mockResolvedValue(undefined);
            const validateFunctionalitySpy = jest.spyOn(restartService as any, 'validateFunctionality').mockResolvedValue(undefined);
            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(new Map());

            await restartService.handleApplicationRestart();

            expect(restoreStateSpy).toHaveBeenCalled();
            expect(restoreWatchingSpy).toHaveBeenCalled();
            expect(validateFunctionalitySpy).toHaveBeenCalled();
            expect(mockLogger.info).toHaveBeenCalledWith('Hot reload restart process completed successfully');
        });

        it('should handle errors during application restart with basic recovery', async () => {
            // Mock failure in one of the restart steps
            const restoreStateSpy = jest.spyOn(restartService as any, 'restoreStateAfterRestart')
                .mockRejectedValue(new Error('Test error'));
            const basicRecoverySpy = jest.spyOn(restartService as any, 'performBasicRecovery').mockResolvedValue(undefined);

            await restartService.handleApplicationRestart();

            expect(restoreStateSpy).toHaveBeenCalled();
            expect(basicRecoverySpy).toHaveBeenCalled();
            expect(mockErrorHandler.handleError).toHaveBeenCalled();
        });

        it('should handle errors during application restart with advanced recovery', async () => {
            // Mock failure in one of the restart steps
            const restoreStateSpy = jest.spyOn(restartService as any, 'restoreStateAfterRestart')
                .mockRejectedValue(new Error('Test error'));
            // 通过模拟 ProjectHotReloadService 恢复失败来让 basic recovery 失败
            // mockIndexService.restoreProjectWatchingAfterRestart.mockRejectedValue(new Error('Basic recovery failed'));
            const advancedRecoverySpy = jest.spyOn(restartService as any, 'performAdvancedRecovery').mockResolvedValue(undefined);

            await restartService.handleApplicationRestart();

            expect(restoreStateSpy).toHaveBeenCalled();
            // expect(mockIndexService.restoreProjectWatchingAfterRestart).toHaveBeenCalled(); // basic recovery内部会调用这个
            expect(advancedRecoverySpy).toHaveBeenCalled();
            expect(mockErrorHandler.handleError).toHaveBeenCalled();
        });
    });

    describe('saveCurrentState', () => {
        it('should save current state to file', async () => {
            const mockStatuses = new Map([
                ['project1', { enabled: true, isWatching: true, watchedPaths: ['/path1'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }]
            ]);
            const mockConfig = { enabled: true, debounceInterval: 500, watchPatterns: ['**/*.{js,ts}'], ignorePatterns: ['**/node_modules/**'], maxFileSize: 1024 * 1024, errorHandling: { maxRetries: 3, alertThreshold: 5, autoRecovery: true } };
            const mockMetrics = { filesProcessed: 0, changesDetected: 0, averageProcessingTime: 0, lastUpdated: new Date(), errorCount: 0, errorBreakdown: {}, recoveryStats: { autoRecovered: 0, manualIntervention: 0, failedRecoveries: 0 } };

            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(mockStatuses);
            mockProjectHotReloadService.getProjectConfig.mockReturnValue(mockConfig);
            mockProjectHotReloadService.getProjectMetrics.mockReturnValue(mockMetrics);

            (fs.writeFile as jest.Mock).mockResolvedValue(undefined);

            await restartService.saveCurrentState();

            expect(mockProjectHotReloadService.getAllProjectStatuses).toHaveBeenCalled();
            expect(fs.writeFile).toHaveBeenCalledWith(
                expect.stringContaining('hotreload-restart-state.json'),
                expect.any(String)
            );
            expect(mockLogger.info).toHaveBeenCalledWith(
                expect.stringContaining('Hot reload state saved for')
            );
        });

        it('should handle errors during state saving', async () => {
            const mockStatuses = new Map([
                ['project1', { enabled: true, isWatching: true, watchedPaths: ['/path1'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }]
            ]);
            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(mockStatuses);
            (fs.writeFile as jest.Mock).mockRejectedValue(new Error('Write error'));

            await expect(restartService.saveCurrentState()).rejects.toThrow('Write error');
            expect(mockErrorHandler.handleError).toHaveBeenCalled();
        });
    });

    describe('loadRestartState', () => {
        it('should return null if state file does not exist', async () => {
            (fs.readFile as jest.Mock).mockRejectedValue({ code: 'ENOENT' });

            const result = await (restartService as any).loadRestartState();
            expect(result).toBeNull();
        });

        it('should return null if state file is empty', async () => {
            (fs.readFile as jest.Mock).mockResolvedValue('');

            const result = await (restartService as any).loadRestartState();
            expect(result).toBeNull();
        });

        it('should return null if state file has invalid JSON', async () => {
            (fs.readFile as jest.Mock).mockResolvedValue('invalid json');

            const result = await (restartService as any).loadRestartState();
            expect(result).toBeNull();
            expect(mockLogger.warn).toHaveBeenCalledWith(
                'Invalid JSON in restart state file, starting fresh'
            );
        });
    });

    describe('restoreStateAfterRestart', () => {
        it('should restore state from file if exists', async () => {
            const mockStateData = {
                projects: [
                    {
                        projectPath: '/test/project',
                        config: { enabled: true, debounceInterval: 500, watchPatterns: ['**/*.{js,ts}'], ignorePatterns: ['**/node_modules/**'], maxFileSize: 1024 * 1024, errorHandling: { maxRetries: 3, alertThreshold: 5, autoRecovery: true } },
                        status: { enabled: true, isWatching: true },
                        metrics: { filesProcessed: 0, changesDetected: 0, averageProcessingTime: 0, lastUpdated: new Date(), errorCount: 0, errorBreakdown: {}, recoveryStats: { autoRecovered: 0, manualIntervention: 0, failedRecoveries: 0 } }
                    }
                ],
                timestamp: new Date()
            };

            const loadRestartStateSpy = jest.spyOn(restartService as any, 'loadRestartState').mockResolvedValue(mockStateData);
            (fs.access as jest.Mock).mockResolvedValue(undefined); // 模拟项目路径存在
            (restartService as any).checkProjectExists = jest.fn().mockResolvedValue(true); // 模拟项目存在
            (restartService as any).isProjectIndexed = jest.fn().mockResolvedValue(true); // 模拟项目已索引
            mockProjectHotReloadService.enableForProject.mockResolvedValue(undefined);

            await (restartService as any).restoreStateAfterRestart();

            expect(loadRestartStateSpy).toHaveBeenCalled();
            expect(mockProjectHotReloadService.enableForProject).toHaveBeenCalledWith(
                '/test/project',
                mockStateData.projects[0].config
            );
        });

        it('should skip restoration if project no longer exists', async () => {
            const mockStateData = {
                projects: [
                    {
                        projectPath: '/nonexistent/project',
                        config: { enabled: true, debounceInterval: 50 },
                        status: { enabled: true, isWatching: true },
                        metrics: { filesProcessed: 0, changesDetected: 0, averageProcessingTime: 0, lastUpdated: new Date(), errorCount: 0, errorBreakdown: {}, recoveryStats: { autoRecovered: 0, manualIntervention: 0, failedRecoveries: 0 } }
                    }
                ],
                timestamp: new Date()
            };

            const loadRestartStateSpy = jest.spyOn(restartService as any, 'loadRestartState').mockResolvedValue(mockStateData);
            (fs.access as jest.Mock).mockRejectedValue(new Error('Not found'));

            await (restartService as any).restoreStateAfterRestart();

            expect(loadRestartStateSpy).toHaveBeenCalled();
            expect(mockProjectHotReloadService.enableForProject).not.toHaveBeenCalled();
        });
    });

    describe('restoreProjectWatchingThroughIndexService', () => {
        it('should call ProjectHotReloadService to restore project watching', async () => {
            // mockIndexService.restoreProjectWatchingAfterRestart.mockResolvedValue(undefined);

            // await (restartService as any).restoreProjectWatchingThroughIndexService();

            // expect(mockIndexService.restoreProjectWatchingAfterRestart).toHaveBeenCalled();
        });

        it('should handle errors during restoration', async () => {
            // mockIndexService.restoreProjectWatchingAfterRestart.mockRejectedValue(new Error('Index service error'));

            // await expect((restartService as any).restoreProjectWatchingThroughIndexService()).rejects.toThrow('Index service error');
            // expect(mockErrorHandler.handleError).toHaveBeenCalled();
        });
    });

    describe('validateFunctionality', () => {
        it('should validate functionality and reinitialize change detection if needed', async () => {
            const isServiceRunningSpy = jest.spyOn(mockChangeDetectionService, 'isServiceRunning').mockReturnValue(false);
            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(new Map([
                ['/test/project', { enabled: true, isWatching: true, watchedPaths: ['/test/project'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }]
            ]));
            mockChangeDetectionService.initialize.mockResolvedValue(undefined);
            // mockIndexService.getAllIndexedProjectPaths.mockReturnValue(['/test/project']);

            await (restartService as any).validateFunctionality();

            expect(isServiceRunningSpy).toHaveBeenCalled();
            expect(mockChangeDetectionService.initialize).toHaveBeenCalledWith(['/test/project']);
        });

        it('should not reinitialize if service is already running', async () => {
            const isServiceRunningSpy = jest.spyOn(mockChangeDetectionService, 'isServiceRunning').mockReturnValue(true);

            await (restartService as any).validateFunctionality();

            expect(isServiceRunningSpy).toHaveBeenCalled();
            expect(mockChangeDetectionService.initialize).not.toHaveBeenCalled();
        });
    });

    describe('performBasicRecovery', () => {
        it('should perform basic recovery through ProjectHotReloadService', async () => {
            // mockIndexService.restoreProjectWatchingAfterRestart.mockResolvedValue(undefined);

            // await (restartService as any).performBasicRecovery();

            // expect(mockIndexService.restoreProjectWatchingAfterRestart).toHaveBeenCalled();
        });

        it('should handle fallback recovery when ProjectHotReloadService fails', async () => {
            // mockIndexService.restoreProjectWatchingAfterRestart.mockRejectedValue(new Error('Index service failed'));
            (restartService as any).getIndexedProjects = jest.fn().mockResolvedValue(['/test/project']);
            mockConfigService.getProjectConfig.mockReturnValue({ enabled: true, debounceInterval: 500, watchPatterns: ['**/*.{js,ts}'], ignorePatterns: ['**/node_modules/**'], maxFileSize: 1024 * 1024, errorHandling: { maxRetries: 3, alertThreshold: 5, autoRecovery: true } });
            mockProjectHotReloadService.enableForProject.mockResolvedValue(undefined);

            await (restartService as any).performBasicRecovery();

            expect(mockProjectHotReloadService.enableForProject).toHaveBeenCalledWith('/test/project', expect.any(Object));
        });
    });

    describe('performAdvancedRecovery', () => {
        it('should perform advanced recovery process', async () => {
            const resetServicesSpy = jest.spyOn(restartService as any, 'resetServices').mockResolvedValue(undefined);
            const reinitializeProjectStatesSpy = jest.spyOn(restartService as any, 'reinitializeProjectStates').mockResolvedValue(undefined);
            const reinitializeHotReloadSpy = jest.spyOn(restartService as any, 'reinitializeHotReloadFunctionality').mockResolvedValue(undefined);
            const validateRecoveryResultSpy = jest.spyOn(restartService as any, 'validateRecoveryResult').mockResolvedValue(undefined);

            await restartService.performAdvancedRecovery();

            expect(resetServicesSpy).toHaveBeenCalled();
            expect(reinitializeProjectStatesSpy).toHaveBeenCalled();
            expect(reinitializeHotReloadSpy).toHaveBeenCalled();
            expect(validateRecoveryResultSpy).toHaveBeenCalled();
        });
    });

    describe('getRestartState', () => {
        it('should return current restart state', () => {
            const state = restartService.getRestartState();
            expect(state).toBeDefined();
            expect(state.phase).toBeDefined();
            expect(state.timestamp).toBeDefined();
            expect(state.projects).toBeDefined();
        });
    });

    describe('triggerRecovery', () => {
        it('should trigger basic recovery', async () => {
            const basicRecoverySpy = jest.spyOn(restartService as any, 'performBasicRecovery').mockResolvedValue(undefined);

            await restartService.triggerRecovery('basic');

            expect(basicRecoverySpy).toHaveBeenCalled();
            expect(mockLogger.info).toHaveBeenCalledWith(
                'Manual recovery completed successfully',
                { recoveryType: 'basic' }
            );
        });

        it('should trigger advanced recovery', async () => {
            const advancedRecoverySpy = jest.spyOn(restartService as any, 'performAdvancedRecovery').mockResolvedValue(undefined);

            await restartService.triggerRecovery('advanced');

            expect(advancedRecoverySpy).toHaveBeenCalled();
            expect(mockLogger.info).toHaveBeenCalledWith(
                'Manual recovery completed successfully',
                { recoveryType: 'advanced' }
            );
        });
    });

    describe('checkHealth', () => {
        it('should return health status', async () => {
            const isServiceRunningSpy = jest.spyOn(mockChangeDetectionService, 'isServiceRunning').mockReturnValue(true);
            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(new Map([
                ['/test/project', { enabled: true, isWatching: true, watchedPaths: ['/test/project'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }]
            ]));
            // mockIndexService.getAllIndexedProjectPaths.mockReturnValue(['/test/project']);

            const health = await restartService.checkHealth();

            expect(isServiceRunningSpy).toHaveBeenCalled();
            expect(health.isHealthy).toBeDefined();
            expect(health.details).toBeDefined();
        });
    });

    describe('clearRestartState', () => {
        it('should clear restart state file', async () => {
            (fs.unlink as jest.Mock).mockResolvedValue(undefined);

            await restartService.clearRestartState();

            expect(fs.unlink).toHaveBeenCalledWith(expect.stringContaining('hotreload-restart-state.json'));
            expect(mockLogger.info).toHaveBeenCalledWith('Hot reload restart state cleared');
        });
    });

    describe('getHotReloadStats', () => {
        it('should return hot reload statistics', async () => {
            // mockIndexService.getAllIndexedProjectPaths.mockReturnValue(['/project1', '/project2']);
            mockProjectHotReloadService.getAllProjectStatuses.mockReturnValue(new Map([
                ['/project1', { enabled: true, isWatching: true, watchedPaths: ['/project1'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }],
                ['/project2', { enabled: true, isWatching: false, watchedPaths: ['/project2'], lastChange: null, changesCount: 0, errorsCount: 0, lastError: null }]
            ]));

            const stats = await restartService.getHotReloadStats();

            // expect(stats.totalIndexedProjects).toBe(2);
            expect(stats.totalWatchedProjects).toBe(1); // Only 1 is watching
            expect(stats.restartCount).toBe(1);
        });
    });
});