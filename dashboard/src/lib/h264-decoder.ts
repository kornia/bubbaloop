/**
 * H264 video decoder using WebCodecs API
 *
 * WebCodecs provides hardware-accelerated video decoding directly in the browser.
 * Decodes H264 NAL units frame-by-frame and renders to canvas.
 *
 * Supports both Annex B format (start codes, from GStreamer) and
 * AVCC format (length-prefixed, from MP4/RTSP extractors).
 * Format is auto-detected from the first keyframe.
 */

export type FrameCallback = (frame: VideoFrame) => void;

export interface H264DecoderOptions {
  onFrame: FrameCallback;
  onError?: (error: Error) => void;
}

interface SPSInfo {
  profileIdc: number;
  constraintFlags: number;
  levelIdc: number;
  spsData: Uint8Array;
  ppsData?: Uint8Array;
}

type NalFormat = 'unknown' | 'annexb' | 'avcc';

interface NalUnit {
  type: number;
  data: Uint8Array;
  offset: number;
}

export class H264Decoder {
  private decoder: VideoDecoder | null = null;
  private options: H264DecoderOptions;
  private initialized = false;
  private waitingForKeyframe = true;
  private spsInfo: SPSInfo | null = null;
  private errorCount = 0;
  private maxErrors = 3;
  private nalFormat: NalFormat = 'unknown';
  private framesReceived = 0;
  private lastDiagnosticLog = 0;

  constructor(options: H264DecoderOptions) {
    this.options = options;
  }

  /**
   * Check if WebCodecs is supported
   */
  static isSupported(): boolean {
    return typeof VideoDecoder !== 'undefined';
  }

  /**
   * Detect whether data is Annex B (start codes) or AVCC (length-prefixed).
   */
  private detectFormat(data: Uint8Array): NalFormat {
    if (data.length < 5) return 'unknown';

    // Check for Annex B start codes
    if (data[0] === 0 && data[1] === 0) {
      if (data[2] === 1) return 'annexb'; // 3-byte start code
      if (data[2] === 0 && data[3] === 1) return 'annexb'; // 4-byte start code
    }

    // Check for AVCC: first 4 bytes are big-endian NAL length
    const nalLen = (data[0] << 24) | (data[1] << 16) | (data[2] << 8) | data[3];
    if (nalLen > 0 && nalLen <= data.length - 4) {
      // Verify the NAL header byte after the length looks valid
      const nalHeader = data[4];
      const forbiddenBit = (nalHeader >> 7) & 1;
      const nalType = nalHeader & 0x1f;
      if (forbiddenBit === 0 && nalType > 0 && nalType <= 23) {
        return 'avcc';
      }
    }

    return 'unknown';
  }

  /**
   * Parse NAL units from Annex B format (start code delimited).
   */
  private parseNalUnitsAnnexB(data: Uint8Array): NalUnit[] {
    const nalUnits: NalUnit[] = [];
    let i = 0;

    while (i < data.length - 2) {
      // Find start code (0x00 0x00 0x01 or 0x00 0x00 0x00 0x01)
      if (data[i] === 0 && data[i + 1] === 0) {
        let nalStart = -1;

        if (data[i + 2] === 1) {
          nalStart = i + 3;
        } else if (data[i + 2] === 0 && i + 3 < data.length && data[i + 3] === 1) {
          nalStart = i + 4;
        }

        if (nalStart >= 0 && nalStart < data.length) {
          // Find the end of this NAL (next start code or end of data)
          let nalEnd = data.length;
          for (let j = nalStart + 1; j < data.length - 2; j++) {
            if (data[j] === 0 && data[j + 1] === 0 && (data[j + 2] === 1 || (data[j + 2] === 0 && j + 3 < data.length && data[j + 3] === 1))) {
              nalEnd = j;
              break;
            }
          }

          const nalType = data[nalStart] & 0x1f;
          nalUnits.push({
            type: nalType,
            data: data.slice(nalStart, nalEnd),
            offset: i,
          });

          i = nalEnd;
          continue;
        }
      }
      i++;
    }

    return nalUnits;
  }

  /**
   * Parse NAL units from AVCC format (4-byte big-endian length prefixed).
   */
  private parseNalUnitsAVCC(data: Uint8Array): NalUnit[] {
    const nalUnits: NalUnit[] = [];
    let i = 0;

    while (i + 4 < data.length) {
      const nalLen = (data[i] << 24) | (data[i + 1] << 16) | (data[i + 2] << 8) | data[i + 3];

      if (nalLen <= 0 || nalLen > data.length - i - 4) {
        break; // Invalid length
      }

      const nalStart = i + 4;
      const nalEnd = nalStart + nalLen;
      const nalType = data[nalStart] & 0x1f;

      nalUnits.push({
        type: nalType,
        data: data.slice(nalStart, nalEnd),
        offset: i,
      });

      i = nalEnd;
    }

    return nalUnits;
  }

  /**
   * Parse NAL units, auto-detecting format on first successful parse.
   */
  private parseNalUnits(data: Uint8Array): NalUnit[] {
    // Use cached format if already detected
    if (this.nalFormat === 'annexb') {
      return this.parseNalUnitsAnnexB(data);
    }
    if (this.nalFormat === 'avcc') {
      return this.parseNalUnitsAVCC(data);
    }

    // Auto-detect format
    const detected = this.detectFormat(data);

    if (detected === 'annexb') {
      const units = this.parseNalUnitsAnnexB(data);
      if (units.length > 0) {
        this.nalFormat = 'annexb';
        console.log(`[H264Decoder] Detected Annex B format (start code delimited)`);
        return units;
      }
    }

    if (detected === 'avcc') {
      const units = this.parseNalUnitsAVCC(data);
      if (units.length > 0) {
        this.nalFormat = 'avcc';
        console.log(`[H264Decoder] Detected AVCC format (length-prefixed)`);
        return units;
      }
    }

    // Try both parsers as fallback
    const annexB = this.parseNalUnitsAnnexB(data);
    if (annexB.length > 0) {
      this.nalFormat = 'annexb';
      console.log(`[H264Decoder] Detected Annex B format (fallback)`);
      return annexB;
    }

    const avcc = this.parseNalUnitsAVCC(data);
    if (avcc.length > 0) {
      this.nalFormat = 'avcc';
      console.log(`[H264Decoder] Detected AVCC format (fallback)`);
      return avcc;
    }

    return [];
  }

  /**
   * Convert AVCC data to Annex B by replacing length prefixes with start codes.
   * WebCodecs in Annex B mode (no description) expects start codes.
   */
  private avccToAnnexB(data: Uint8Array): Uint8Array {
    const nalUnits = this.parseNalUnitsAVCC(data);
    if (nalUnits.length === 0) return data;

    // Calculate output size: each NAL gets a 4-byte start code + its data
    let totalSize = 0;
    for (const nal of nalUnits) {
      totalSize += 4 + nal.data.length;
    }

    const output = new Uint8Array(totalSize);
    let offset = 0;

    for (const nal of nalUnits) {
      // Write 4-byte start code
      output[offset] = 0;
      output[offset + 1] = 0;
      output[offset + 2] = 0;
      output[offset + 3] = 1;
      offset += 4;
      // Write NAL data
      output.set(nal.data, offset);
      offset += nal.data.length;
    }

    return output;
  }

  /**
   * Extract SPS and PPS from NAL units
   */
  private extractParameterSets(data: Uint8Array): SPSInfo | null {
    const nalUnits = this.parseNalUnits(data);

    let sps: Uint8Array | null = null;
    let pps: Uint8Array | null = null;

    for (const nal of nalUnits) {
      if (nal.type === 7 && !sps) { // SPS
        sps = nal.data;
      } else if (nal.type === 8 && !pps) { // PPS
        pps = nal.data;
      }
    }

    if (sps && sps.length >= 4) {
      return {
        profileIdc: sps[1],
        constraintFlags: sps[2],
        levelIdc: sps[3],
        spsData: sps,
        ppsData: pps || undefined,
      };
    }

    return null;
  }

  /**
   * Initialize the decoder with basic configuration
   */
  async init(): Promise<void> {
    if (!H264Decoder.isSupported()) {
      throw new Error('WebCodecs VideoDecoder not supported');
    }

    this.close();

    this.decoder = new VideoDecoder({
      output: (frame: VideoFrame) => {
        this.errorCount = 0; // Reset error count on successful decode
        this.options.onFrame(frame);
      },
      error: (e: DOMException) => {
        console.error('[H264Decoder] Decoder error:', e.name, e.message);
        this.errorCount++;

        if (this.errorCount >= this.maxErrors) {
          console.error('[H264Decoder] Too many errors, stopping');
          this.options.onError?.(new Error(`${e.name}: ${e.message}`));
        } else {
          // Try to recover by waiting for next keyframe
          console.log('[H264Decoder] Attempting recovery, waiting for next keyframe');
          this.waitingForKeyframe = true;
        }
      },
    });

    this.initialized = true;
    this.waitingForKeyframe = true;
    this.spsInfo = null;
    this.nalFormat = 'unknown';
    this.errorCount = 0;
    this.framesReceived = 0;
    this.lastDiagnosticLog = 0;

    console.log('[H264Decoder] Decoder created, waiting for SPS/PPS');
  }

  /**
   * Configure decoder with detected codec parameters
   *
   * Important: We do NOT provide the AVCC description because our data is in Annex B format.
   * When description is provided, WebCodecs expects AVCC format (NAL length prefixes).
   * When description is omitted, WebCodecs expects Annex B format (start codes).
   */
  private async configureDecoder(spsInfo: SPSInfo): Promise<boolean> {
    if (!this.decoder || this.decoder.state === 'closed') {
      return false;
    }

    const codec = `avc1.${spsInfo.profileIdc.toString(16).padStart(2, '0')}${spsInfo.constraintFlags.toString(16).padStart(2, '0')}${spsInfo.levelIdc.toString(16).padStart(2, '0')}`;
    console.log(`[H264Decoder] Configuring with codec: ${codec}`);

    // Configure WITHOUT description for Annex B format data
    // When description is omitted, WebCodecs expects Annex B (start codes)
    // When description is provided, WebCodecs expects AVCC (length prefixes)
    const config: VideoDecoderConfig = {
      codec,
      optimizeForLatency: true,
    };

    try {
      const support = await VideoDecoder.isConfigSupported(config);
      if (support.supported) {
        // Reset decoder if it's already configured
        if (this.decoder.state === 'configured') {
          this.decoder.reset();
        }
        this.decoder.configure(config);
        console.log(`[H264Decoder] Configured for Annex B format (no description)`);
        return true;
      } else {
        console.error(`[H264Decoder] Codec ${codec} not supported`);
      }
    } catch (e) {
      console.error('[H264Decoder] Configuration error:', e);
    }

    return false;
  }

  /**
   * Check if data contains a keyframe (IDR NAL unit)
   */
  private containsKeyframe(data: Uint8Array): boolean {
    const nalUnits = this.parseNalUnits(data);
    return nalUnits.some(nal => nal.type === 5); // IDR slice
  }

  /**
   * Log diagnostic info when stuck waiting for keyframe
   */
  private logDiagnostic(data: Uint8Array): void {
    const now = Date.now();
    // Log at most every 3 seconds
    if (now - this.lastDiagnosticLog < 3000) return;
    this.lastDiagnosticLog = now;

    const first16 = Array.from(data.slice(0, 16))
      .map(b => b.toString(16).padStart(2, '0'))
      .join(' ');

    // Try to parse with both formats to diagnose
    const annexBNals = this.parseNalUnitsAnnexB(data);
    const avccNals = this.parseNalUnitsAVCC(data);

    const annexBTypes = annexBNals.map(n => n.type);
    const avccTypes = avccNals.map(n => n.type);

    console.warn(
      `[H264Decoder] Waiting for keyframe — received ${this.framesReceived} frames, ` +
      `last: ${data.length} bytes, format: ${this.nalFormat}, ` +
      `first16: [${first16}], ` +
      `annexB NALs: [${annexBTypes.join(',')}], ` +
      `avcc NALs: [${avccTypes.join(',')}]`
    );
  }

  /**
   * Decode H264 data
   * @param data - H264 NAL units in Annex B or AVCC format (auto-detected)
   * @param timestamp - Timestamp in microseconds
   */
  async decode(data: Uint8Array, timestamp: number): Promise<void> {
    if (!this.decoder || !this.initialized) {
      return;
    }

    this.framesReceived++;

    // Check if decoder is in a bad state
    if (this.decoder.state === 'closed') {
      console.log('[H264Decoder] Decoder closed, reinitializing');
      await this.init();
      return;
    }

    const isKeyframe = this.containsKeyframe(data);

    // Wait for a keyframe to start decoding
    if (this.waitingForKeyframe) {
      if (!isKeyframe) {
        this.logDiagnostic(data);
        return;
      }

      console.log(`[H264Decoder] Keyframe received (${data.length} bytes, format: ${this.nalFormat})`);

      // Extract SPS/PPS and configure decoder
      const spsInfo = this.extractParameterSets(data);
      if (spsInfo) {
        this.spsInfo = spsInfo;

        // Log NAL types for debugging
        const nalUnits = this.parseNalUnits(data);
        const nalTypes = nalUnits.map(n => n.type);
        console.log(`[H264Decoder] NAL types in keyframe: ${nalTypes.join(', ')} (7=SPS, 8=PPS, 5=IDR, 9=AUD, 6=SEI)`);

        const configured = await this.configureDecoder(spsInfo);
        if (!configured) {
          console.error('[H264Decoder] Failed to configure decoder');
          return;
        }
      } else {
        console.warn('[H264Decoder] No SPS found in keyframe, cannot configure decoder');
        return;
      }

      this.waitingForKeyframe = false;
    }

    // Verify decoder is configured
    if (this.decoder.state !== 'configured') {
      console.warn('[H264Decoder] Decoder not configured, waiting for keyframe');
      this.waitingForKeyframe = true;
      return;
    }

    // Convert AVCC to Annex B if needed (WebCodecs without description expects Annex B)
    const decodeData = this.nalFormat === 'avcc' ? this.avccToAnnexB(data) : data;

    try {
      const chunk = new EncodedVideoChunk({
        type: isKeyframe ? 'key' : 'delta',
        timestamp,
        data: decodeData,
      });

      this.decoder.decode(chunk);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      console.error('[H264Decoder] EncodedVideoChunk/decode error:', errorMsg);

      // Log data info for debugging on first error
      if (this.errorCount === 0) {
        console.error('[H264Decoder] Failing frame info:', {
          isKeyframe,
          timestamp,
          dataLength: data.length,
          format: this.nalFormat,
          first16Bytes: Array.from(data.slice(0, 16)).map(b => b.toString(16).padStart(2, '0')).join(' '),
        });
      }

      this.errorCount++;

      // On error, wait for next keyframe to resync
      if (this.errorCount >= 2) {
        this.waitingForKeyframe = true;
      }
    }
  }

  /**
   * Flush pending frames
   */
  async flush(): Promise<void> {
    if (this.decoder?.state === 'configured') {
      try {
        await this.decoder.flush();
      } catch (e) {
        console.warn('[H264Decoder] Flush error:', e);
      }
    }
  }

  /**
   * Reset the decoder (call after errors or stream discontinuity)
   */
  async reset(): Promise<void> {
    this.waitingForKeyframe = true;
    this.errorCount = 0;

    if (this.decoder?.state === 'configured') {
      try {
        this.decoder.reset();
      } catch (e) {
        console.warn('[H264Decoder] Reset error:', e);
      }
    }

    // Re-configure if we have SPS info
    if (this.spsInfo && this.decoder?.state === 'unconfigured') {
      await this.configureDecoder(this.spsInfo);
    }
  }

  /**
   * Close the decoder
   */
  close(): void {
    if (this.decoder && this.decoder.state !== 'closed') {
      try {
        this.decoder.close();
      } catch (e) {
        console.warn('[H264Decoder] Close error:', e);
      }
    }
    this.decoder = null;
    this.initialized = false;
    this.waitingForKeyframe = true;
    this.spsInfo = null;
    this.nalFormat = 'unknown';
    this.errorCount = 0;
    this.framesReceived = 0;
  }

  get isInitialized(): boolean {
    return this.initialized && this.decoder?.state === 'configured';
  }
}
