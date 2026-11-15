//! The byte stream, Ogg pages and packets, and the bit reader (stb_vorbis's get8 .. prep_huffman).

use crate::VorbisError;

/// "No more bits in this packet" (stb_vorbis's EOP and INVALID_BITS).
pub(crate) const EOP: i32 = -1;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProbedPage {
    pub page_start: u32,
    pub page_end: u32,
    pub last_decoded_sample: u32,
}

/// Everything about where the decoder is in the file.
pub(crate) struct Io<'a> {
    pub data: &'a [u8],
    /// Offset of the next byte. A skip may put it past the end.
    pub pos: usize,
    pub eof: bool,
    pub error: Option<VorbisError>,

    pub p_first: ProbedPage,
    pub first_decode: bool,

    // The current page.
    pub last_page: u32,
    pub segment_count: i32,
    pub segments: [u8; 255],
    pub page_flag: u8,
    pub next_seg: i32,
    pub last_seg: bool,
    /// The segment number of the last segment of the packet being read.
    pub last_seg_which: i32,
    pub end_seg_with_known_loc: i32,
    pub known_loc_for_packet: u32,

    // The current packet and its bits.
    pub bytes_in_seg: u8,
    pub acc: u32,
    pub valid_bits: i32,
    pub packet_bytes: i32,
}

impl<'a> Io<'a> {
    pub fn new(data: &'a [u8]) -> Io<'a> {
        Io {
            data,
            pos: 0,
            eof: false,
            error: None,
            p_first: ProbedPage::default(),
            first_decode: false,
            last_page: 0,
            segment_count: 0,
            segments: [0; 255],
            page_flag: 0,
            next_seg: 0,
            last_seg: false,
            last_seg_which: 0,
            end_seg_with_known_loc: 0,
            known_loc_for_packet: 0,
            bytes_in_seg: 0,
            acc: 0,
            valid_bits: 0,
            packet_bytes: 0,
        }
    }

    /// Records an error and gives stb_vorbis's "failed" (0).
    pub fn error(&mut self, e: VorbisError) -> bool {
        self.error = Some(e);
        false
    }

    pub fn stream_len(&self) -> u32 {
        self.data.len() as u32
    }

    pub fn get8(&mut self) -> u8 {
        match self.data.get(self.pos) {
            Some(&byte) => {
                self.pos += 1;
                byte
            }
            None => {
                self.eof = true;
                0
            }
        }
    }

    pub fn get32(&mut self) -> u32 {
        let mut x = u32::from(self.get8());
        x += u32::from(self.get8()) << 8;
        x += u32::from(self.get8()) << 16;
        x += u32::from(self.get8()) << 24;
        x
    }

    /// The next `n` bytes, or nothing (and end of file) if there aren't that many.
    pub fn getn(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&end| end <= self.data.len());
        match end {
            Some(end) => {
                let bytes = &self.data[self.pos..end];
                self.pos = end;
                Some(bytes)
            }
            None => {
                self.eof = true;
                None
            }
        }
    }

    pub fn skip(&mut self, n: i64) {
        self.pos = (self.pos as i64).saturating_add(n).max(0) as usize;
        if self.pos >= self.data.len() {
            self.eof = true;
        }
    }

    pub fn set_file_offset(&mut self, loc: u32) -> bool {
        self.eof = false;
        if loc as usize >= self.data.len() {
            self.pos = self.data.len();
            self.eof = true;
            false
        } else {
            self.pos = loc as usize;
            true
        }
    }

    pub fn file_offset(&self) -> u32 {
        self.pos as u32
    }

    fn capture_pattern(&mut self) -> bool {
        self.get8() == 0x4f && self.get8() == 0x67 && self.get8() == 0x67 && self.get8() == 0x53
    }

    pub fn start_page_no_capturepattern(&mut self) -> bool {
        if self.first_decode {
            self.p_first.page_start = self.file_offset().wrapping_sub(4);
        }
        // stream structure version
        if self.get8() != 0 {
            return self.error(VorbisError::InvalidStreamStructureVersion);
        }
        // header flag
        self.page_flag = self.get8();
        // absolute granule position
        let loc0 = self.get32();
        let loc1 = self.get32();
        // stream serial number -- vorbis doesn't care, although ogg does
        self.get32();
        // page sequence number
        self.last_page = self.get32();
        // CRC32
        self.get32();
        // page_segments
        self.segment_count = i32::from(self.get8());
        let count = self.segment_count as usize;
        match self.getn(count) {
            Some(segments) => self.segments[..count].copy_from_slice(segments),
            None => return self.error(VorbisError::UnexpectedEof),
        }
        // assume we _don't_ know the sample position of any segments
        self.end_seg_with_known_loc = -2;
        if loc0 != !0 || loc1 != !0 {
            // determine which packet is the last one that will complete
            if let Some(i) = self.segments[..count].iter().rposition(|&segment| segment < 255) {
                // 'i' is now the index of the _last_ segment of a packet that ends
                self.end_seg_with_known_loc = i as i32;
                self.known_loc_for_packet = loc0;
            }
        }
        if self.first_decode {
            let mut len = 0u32;
            for &segment in &self.segments[..count] {
                len += u32::from(segment);
            }
            len += 27 + count as u32;
            self.p_first.page_end = self.p_first.page_start.wrapping_add(len);
            self.p_first.last_decoded_sample = loc0;
        }
        self.next_seg = 0;
        true
    }

    pub fn start_page(&mut self) -> bool {
        if !self.capture_pattern() {
            return self.error(VorbisError::MissingCapturePattern);
        }
        self.start_page_no_capturepattern()
    }

    pub fn start_packet(&mut self) -> bool {
        while self.next_seg == -1 {
            if !self.start_page() {
                return false;
            }
            if self.page_flag & 1 != 0 {
                return self.error(VorbisError::ContinuedPacketFlagInvalid);
            }
        }
        self.last_seg = false;
        self.valid_bits = 0;
        self.packet_bytes = 0;
        self.bytes_in_seg = 0;
        true
    }

    pub fn maybe_start_packet(&mut self) -> bool {
        if self.next_seg == -1 {
            let x = self.get8();
            if self.eof {
                return false; // EOF at page boundary is not an error!
            }
            if x != 0x4f || self.get8() != 0x67 || self.get8() != 0x67 || self.get8() != 0x53 {
                return self.error(VorbisError::MissingCapturePattern);
            }
            if !self.start_page_no_capturepattern() {
                return false;
            }
            if self.page_flag & 1 != 0 {
                // set up enough state that we can read this packet if we want,
                // e.g. during recovery
                self.last_seg = false;
                self.bytes_in_seg = 0;
                return self.error(VorbisError::ContinuedPacketFlagInvalid);
            }
        }
        self.start_packet()
    }

    pub fn next_segment(&mut self) -> i32 {
        if self.last_seg {
            return 0;
        }
        if self.next_seg == -1 {
            self.last_seg_which = self.segment_count - 1; // in case start_page fails
            if !self.start_page() {
                self.last_seg = true;
                return 0;
            }
            if self.page_flag & 1 == 0 {
                self.error(VorbisError::ContinuedPacketFlagInvalid);
                return 0;
            }
        }
        let len = i32::from(self.segments.get(self.next_seg as usize).copied().unwrap_or(0));
        self.next_seg += 1;
        if len < 255 {
            self.last_seg = true;
            self.last_seg_which = self.next_seg - 1;
        }
        if self.next_seg >= self.segment_count {
            self.next_seg = -1;
        }
        self.bytes_in_seg = len as u8;
        len
    }

    pub fn get8_packet_raw(&mut self) -> i32 {
        if self.bytes_in_seg == 0 {
            if self.last_seg || self.next_segment() == 0 {
                return EOP;
            }
        }
        self.bytes_in_seg = self.bytes_in_seg.wrapping_sub(1);
        self.packet_bytes += 1;
        i32::from(self.get8())
    }

    pub fn get8_packet(&mut self) -> i32 {
        let x = self.get8_packet_raw();
        self.valid_bits = 0;
        x
    }

    pub fn get32_packet(&mut self) -> i32 {
        let mut x = self.get8_packet() as u32;
        x = x.wrapping_add((self.get8_packet() << 8) as u32);
        x = x.wrapping_add((self.get8_packet() << 16) as u32);
        x = x.wrapping_add((self.get8_packet() as u32) << 24);
        x as i32
    }

    pub fn flush_packet(&mut self) {
        while self.get8_packet_raw() != EOP {}
    }

    // @OPTIMIZE: this is the secondary bit decoder, so it's probably not as important
    // as the huffman decoder?
    pub fn get_bits(&mut self, n: i32) -> u32 {
        if self.valid_bits < 0 {
            return 0;
        }
        if self.valid_bits < n {
            if n > 24 {
                // the accumulator technique below would not work correctly in this case
                let mut z = self.get_bits(24);
                z = z.wrapping_add(self.get_bits(n - 24) << 24);
                return z;
            }
            if self.valid_bits == 0 {
                self.acc = 0;
            }
            while self.valid_bits < n {
                let z = self.get8_packet_raw();
                if z == EOP {
                    self.valid_bits = EOP;
                    return 0;
                }
                self.acc = self.acc.wrapping_add((z as u32) << self.valid_bits);
                self.valid_bits += 8;
            }
        }
        let z = self.acc & 1u32.checked_shl(n as u32).map_or(!0, |bit| bit - 1);
        self.acc = self.acc.checked_shr(n as u32).unwrap_or(0);
        self.valid_bits -= n;
        z
    }

    // @OPTIMIZE: primary accumulator for huffman
    // expand the buffer to as many bits as possible without reading off end of packet
    // it might be nice to allow f->valid_bits and f->acc to be stored in registers,
    // e.g. cache them locally and decode locally
    #[inline]
    pub fn prep_huffman(&mut self) {
        if self.valid_bits <= 24 {
            if self.valid_bits == 0 {
                self.acc = 0;
            }
            loop {
                if self.last_seg && self.bytes_in_seg == 0 {
                    return;
                }
                let z = self.get8_packet_raw();
                if z == EOP {
                    return;
                }
                self.acc = self.acc.wrapping_add((z as u32) << self.valid_bits);
                self.valid_bits += 8;
                if self.valid_bits > 24 {
                    break;
                }
            }
        }
    }
}
