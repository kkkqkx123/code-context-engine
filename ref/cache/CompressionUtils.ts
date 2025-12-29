import * as zlib from 'zlib';

export class CompressionUtils {
  static compress(data: string): Buffer {
    return zlib.gzipSync(Buffer.from(data));
  }
  
  static decompress(data: Buffer): string {
    return zlib.gunzipSync(data).toString();
  }
}