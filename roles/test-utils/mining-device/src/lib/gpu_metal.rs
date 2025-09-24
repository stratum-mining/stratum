#![cfg(all(target_os = "macos", feature = "gpu-metal"))]

use anyhow::Result;
use metal::*;
use std::{mem, time::Instant};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GpuParams {
    midstate: [u32; 8],
    block1_template: [u32; 16],
    target_le: [u32; 8],
    time: u32,
    start_nonce: u32,
    stride: u32,
    count: u32,
}

#[inline]
fn u32_from_be_bytes(b: [u8; 4]) -> u32 {
    u32::from_be_bytes(b)
}
#[inline]
fn u32_from_le_bytes(b: [u8; 4]) -> u32 {
    u32::from_le_bytes(b)
}

pub struct GpuContext {
    device: Device,
    pipe: ComputePipelineState,
    queue: CommandQueue,
}

impl GpuContext {
    pub fn new() -> Result<Self> {
        let device = Device::system_default().ok_or_else(|| anyhow::anyhow!("No Metal device"))?;
        let src = include_str!("../../gpu/sha256d_kernel.metal");
        let opts = CompileOptions::new();
        let lib = device
            .new_library_with_source(src, &opts)
            .map_err(|e| anyhow::anyhow!("Failed to compile Metal shader source: {}", e))?;
        let func = lib
            .get_function("sha256d_scan", None)
            .map_err(|e| anyhow::anyhow!("Failed to get function 'sha256d_scan': {}", e))?;
        let pipe = device
            .new_compute_pipeline_state_with_function(&func)
            .map_err(|e| anyhow::anyhow!("Failed to create compute pipeline state: {}", e))?;
        let queue = device.new_command_queue();
        Ok(Self {
            device,
            pipe,
            queue,
        })
    }

    fn build_common_buffers(
        &self,
        midstate: [u32; 8],
        block1: [u8; 64],
        target_le: [u8; 32],
        time_val: u32,
        start_nonce: u32,
        count: u32,
    ) -> (Buffer, Buffer, Buffer) {
        let mut tmpl_u32 = [0u32; 16];
        for i in 0..16 {
            let mut w = [0u8; 4];
            w.copy_from_slice(&block1[i * 4..i * 4 + 4]);
            tmpl_u32[i] = u32_from_be_bytes(w);
        }
        let mut tgt_le_u32 = [0u32; 8];
        for i in 0..8 {
            let mut w = [0u8; 4];
            w.copy_from_slice(&target_le[i * 4..i * 4 + 4]);
            tgt_le_u32[i] = u32_from_le_bytes(w);
        }
        let params = GpuParams {
            midstate,
            block1_template: tmpl_u32,
            target_le: tgt_le_u32,
            time: time_val,
            start_nonce,
            stride: 1,
            count,
        };
        let params_buf = self.device.new_buffer_with_data(
            &params as *const _ as *const _,
            mem::size_of::<GpuParams>() as u64,
            MTLResourceOptions::CPUCacheModeDefaultCache,
        );
        let found_buf = self.device.new_buffer(
            mem::size_of::<u32>() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        unsafe {
            *(found_buf.contents() as *mut u32) = 0;
        }
        let result_buf = self.device.new_buffer(
            mem::size_of_val(&[0u32; 2]) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        (params_buf, found_buf, result_buf)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scan_for_share(
        &self,
        midstate: [u32; 8],
        block1: [u8; 64],
        target_le: [u8; 32],
        time_val: u32,
        start_nonce: u32,
        threads: u32,
        per_thread: u32,
    ) -> Result<Option<(u32, u32)>> {
        let (params_buf, found_buf, result_buf) = self.build_common_buffers(
            midstate,
            block1,
            target_le,
            time_val,
            start_nonce,
            per_thread,
        );
        let total_threads = threads as u64;
        let grid = MTLSize {
            width: total_threads,
            height: 1,
            depth: 1,
        };
        let tg_width = self.pipe.thread_execution_width();
        let tg_size = MTLSize {
            width: tg_width,
            height: 1,
            depth: 1,
        };

        let cmd = self.queue.new_command_buffer();
        let enc = cmd.new_compute_command_encoder();
        enc.set_compute_pipeline_state(&self.pipe);
        enc.set_buffer(0, Some(&params_buf), 0);
        enc.set_buffer(1, Some(&found_buf), 0);
        enc.set_buffer(2, Some(&result_buf), 0);
        enc.dispatch_threads(grid, tg_size);
        enc.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();

        let found = unsafe { *(found_buf.contents() as *const u32) };
        if found != 0 {
            let vals = unsafe { *(result_buf.contents() as *const [u32; 2]) };
            Ok(Some((vals[0], vals[1])))
        } else {
            Ok(None)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn measure_gpu_mhps(
        &self,
        midstate: [u32; 8],
        block1: [u8; 64],
        target_le: [u8; 32],
        time_val: u32,
        start_nonce: u32,
        threads: u32,
        per_thread: u32,
    ) -> Result<f64> {
        let (params_buf, found_buf, result_buf) = self.build_common_buffers(
            midstate,
            block1,
            target_le,
            time_val,
            start_nonce,
            per_thread,
        );
        let total_threads = threads as u64;
        let grid = MTLSize {
            width: total_threads,
            height: 1,
            depth: 1,
        };
        let tg_width = self.pipe.thread_execution_width();
        let tg_size = MTLSize {
            width: tg_width,
            height: 1,
            depth: 1,
        };

        let start = Instant::now();
        let cmd = self.queue.new_command_buffer();
        let enc = cmd.new_compute_command_encoder();
        enc.set_compute_pipeline_state(&self.pipe);
        enc.set_buffer(0, Some(&params_buf), 0);
        enc.set_buffer(1, Some(&found_buf), 0);
        enc.set_buffer(2, Some(&result_buf), 0);
        enc.dispatch_threads(grid, tg_size);
        enc.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();
        let elapsed = start.elapsed();

        let total_hashes = (threads as u64) * (per_thread as u64);
        let secs = elapsed.as_secs_f64().max(1e-9);
        let mhps = (total_hashes as f64) / secs / 1_000_000.0;
        Ok(mhps)
    }
}

pub fn measure_gpu_mhps(
    midstate: [u32; 8],
    block1: [u8; 64],
    target_le: [u8; 32],
    time_val: u32,
    start_nonce: u32,
    threads: u32,
    per_thread: u32,
) -> Result<f64> {
    let ctx = GpuContext::new()?;
    ctx.measure_gpu_mhps(
        midstate,
        block1,
        target_le,
        time_val,
        start_nonce,
        threads,
        per_thread,
    )
}

pub fn device_info() -> Result<(u64, u64)> {
    let ctx = GpuContext::new()?;
    Ok((
        ctx.pipe.thread_execution_width(),
        ctx.pipe.max_total_threads_per_threadgroup(),
    ))
}
