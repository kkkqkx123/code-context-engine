import { HotReloadError, HotReloadErrorCode } from '../HotReloadError';

describe('HotReloadError', () => {
  describe('HotReloadErrorCode', () => {
    it('should have expected error codes', () => {
      expect(HotReloadErrorCode.FILE_WATCH_FAILED).toBe('FILE_WATCH_FAILED');
      expect(HotReloadErrorCode.CHANGE_DETECTION_FAILED).toBe('CHANGE_DETECTION_FAILED');
      expect(HotReloadErrorCode.INDEX_UPDATE_FAILED).toBe('INDEX_UPDATE_FAILED');
      expect(HotReloadErrorCode.PERMISSION_DENIED).toBe('PERMISSION_DENIED');
      expect(HotReloadErrorCode.FILE_TOO_LARGE).toBe('FILE_TOO_LARGE');
      expect(HotReloadErrorCode.HOT_RELOAD_DISABLED).toBe('HOT_RELOAD_DISABLED');
      expect(HotReloadErrorCode.PROJECT_NOT_FOUND).toBe('PROJECT_NOT_FOUND');
      expect(HotReloadErrorCode.INVALID_CONFIG).toBe('INVALID_CONFIG');
    });
  });

  describe('HotReloadError class', () => {
    it('should create an error with code, message, and context', () => {
      const errorCode = HotReloadErrorCode.CHANGE_DETECTION_FAILED;
      const message = 'Test error message';
      const context = { projectId: 'test-project', filePath: '/path/to/file' };

      const error = new HotReloadError(errorCode, message, context);

      expect(error).toBeInstanceOf(HotReloadError);
      expect(error).toBeInstanceOf(Error);
      expect(error.code).toBe(errorCode);
      expect(error.message).toBe(message);
      expect(error.context).toBe(context);
      expect(error.name).toBe('HotReloadError');
    });

    it('should create an error with code and message only', () => {
      const errorCode = HotReloadErrorCode.PERMISSION_DENIED;
      const message = 'Permission denied error';

      const error = new HotReloadError(errorCode, message);

      expect(error).toBeInstanceOf(HotReloadError);
      expect(error.code).toBe(errorCode);
      expect(error.message).toBe(message);
      expect(error.context).toBeUndefined();
      expect(error.name).toBe('HotReloadError');
    });

    it('should have correct prototype chain', () => {
      const error = new HotReloadError(HotReloadErrorCode.FILE_WATCH_FAILED, 'Test message');

      expect(Object.getPrototypeOf(error)).toBe(HotReloadError.prototype);
      expect(error).toBeInstanceOf(Error);
      expect(error).toBeInstanceOf(HotReloadError);
    });

    it('should work with instanceof operator', () => {
      const error = new HotReloadError(HotReloadErrorCode.INDEX_UPDATE_FAILED, 'Test message');

      expect(error instanceof HotReloadError).toBe(true);
      expect(error instanceof Error).toBe(true);
    });

    it('should have all expected properties', () => {
      const errorCode = HotReloadErrorCode.INVALID_CONFIG;
      const message = 'Invalid configuration error';
      const context = { configProperty: 'test-value' };

      const error = new HotReloadError(errorCode, message, context);

      // Check that it has the expected custom properties
      expect(error.code).toBe(errorCode);
      expect(error.context).toBe(context);

      // Check that it has standard Error properties
      expect(error.message).toBe(message);
      expect(error.name).toBe('HotReloadError');
      expect(error.stack).toBeDefined();
      expect(typeof error.stack).toBe('string');
    });
  });

  describe('error handling compatibility', () => {
    it('should work with standard error handling', () => {
      const error = new HotReloadError(HotReloadErrorCode.FILE_TOO_LARGE, 'File too large');

      // Test that it works with standard JavaScript error handling
      let caughtError: Error | null = null;
      try {
        throw error;
      } catch (e) {
        caughtError = e as Error;
      }

      expect(caughtError).toBe(error);
      expect(caughtError).toBeInstanceOf(HotReloadError);
      expect(caughtError).toBeInstanceOf(Error);
    });

    it('should serialize properly', () => {
      const error = new HotReloadError(HotReloadErrorCode.PROJECT_NOT_FOUND, 'Project not found');

      // Test JSON serialization (only own enumerable properties will be serialized)
      const serialized = JSON.stringify(error);
      expect(serialized).toContain('Project not found');

      // Test string conversion
      expect(String(error)).toContain('Project not found');
    });
  });
});