import { FileSystemTraversal } from '../FileSystemTraversal';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mock logger for testing
const mockLogger = {
  debug: jest.fn(),
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

describe('Binary File Detection', () => {
 let fileSystemTraversal: FileSystemTraversal;

  beforeEach(() => {
    fileSystemTraversal = new FileSystemTraversal(mockLogger as any);
  });

  test('should detect binary files with null bytes', async () => {
    // Create a temporary binary file with null bytes
    const binaryFilePath = path.join(__dirname, 'temp_binary_test.bin');
    
    // Create a buffer with null bytes (typical of binary files)
    const binaryBuffer = Buffer.from([0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0x57, 0x6f, 0x72, 0x6c, 0x64]); // Contains null byte
    await fs.writeFile(binaryFilePath, binaryBuffer);

    try {
      const result = await fileSystemTraversal.isBinaryFile(binaryFilePath);
      expect(result).toBe(true);
    } finally {
      // Clean up
      await fs.unlink(binaryFilePath);
    }
  });

  test('should not detect text files as binary', async () => {
    // Create a temporary text file
    const textFilePath = path.join(__dirname, 'temp_text_test.txt');
    await fs.writeFile(textFilePath, 'This is a test text file.\nIt contains multiple lines.\nBut no null bytes.');

    try {
      const result = await fileSystemTraversal.isBinaryFile(textFilePath);
      expect(result).toBe(false);
    } finally {
      // Clean up
      await fs.unlink(textFilePath);
    }
 });

  test('should handle small files correctly', async () => {
    // Create a small text file
    const smallFilePath = path.join(__dirname, 'temp_small_test.txt');
    await fs.writeFile(smallFilePath, 'Hi'); // Very small file

    try {
      const result = await fileSystemTraversal.isBinaryFile(smallFilePath);
      expect(result).toBe(false);
    } finally {
      // Clean up
      await fs.unlink(smallFilePath);
    }
  });

  test('should handle empty files correctly', async () => {
    // Create an empty file
    const emptyFilePath = path.join(__dirname, 'temp_empty_test.txt');
    await fs.writeFile(emptyFilePath, '');

    try {
      const result = await fileSystemTraversal.isBinaryFile(emptyFilePath);
      expect(result).toBe(false);
    } finally {
      // Clean up
      await fs.unlink(emptyFilePath);
    }
 });

  test('should correctly identify common binary file formats', async () => {
    // Create a simple PNG-like binary content (starts with PNG signature)
    const pngFilePath = path.join(__dirname, 'temp_png_test.png');
    const pngBuffer = Buffer.from([
      0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
      0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x4, 0x52  // IHDR chunk
    ]);
    await fs.writeFile(pngFilePath, pngBuffer);

    try {
      const result = await fileSystemTraversal.isBinaryFile(pngFilePath);
      expect(result).toBe(true);
    } finally {
      // Clean up
      await fs.unlink(pngFilePath);
    }
  });

  test('should handle file read errors gracefully', async () => {
    // Test with a non-existent file
    const nonExistentPath = path.join(__dirname, 'non_existent_file.bin');
    
    const result = await fileSystemTraversal.isBinaryFile(nonExistentPath);
    expect(result).toBe(true); // Should return true on error (safety measure)
  });
});