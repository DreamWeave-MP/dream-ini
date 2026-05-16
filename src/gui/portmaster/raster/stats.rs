// SPDX-License-Identifier: GPL-3.0-only

const TEXTURED_RECT_VECTOR_BLOCK_PX: usize = 16;
pub(in crate::gui::portmaster) const TEXTURED_RECT_SAMPLED_VECTOR_BACKEND_AVAILABLE: usize =
    if cfg!(all(target_arch = "aarch64", target_endian = "little")) {
        1
    } else {
        0
    };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::gui::portmaster) struct RasterStats {
    pub(in crate::gui::portmaster) solid_rect_calls: usize,
    pub(in crate::gui::portmaster) solid_rect_px: usize,
    pub(in crate::gui::portmaster) textured_rect_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_px: usize,
    pub(in crate::gui::portmaster) textured_rect_constant_texel_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_constant_texel_px: usize,
    pub(in crate::gui::portmaster) textured_rect_constant_texel_us: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_px: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_us: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_uv_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_uv_px: usize,
    pub(in crate::gui::portmaster) textured_rect_nonseparable_uv_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_nonseparable_uv_px: usize,
    pub(in crate::gui::portmaster) textured_rect_white_texel_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_white_texel_px: usize,
    pub(in crate::gui::portmaster) textured_rect_uniform_color_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_uniform_color_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_calls: usize,
    pub(in crate::gui::portmaster) solid_triangle_bbox_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_covered_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_span_rows: usize,
    pub(in crate::gui::portmaster) solid_triangle_candidate_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_hint_rows: usize,
    pub(in crate::gui::portmaster) solid_triangle_hint_fallback_rows: usize,
    pub(in crate::gui::portmaster) solid_triangle_hint_build_us: usize,
    pub(in crate::gui::portmaster) solid_triangle_endpoint_search_us: usize,
    pub(in crate::gui::portmaster) solid_triangle_blend_span_us: usize,
    pub(in crate::gui::portmaster) solid_triangle_blend_span_calls: usize,
    pub(in crate::gui::portmaster) solid_triangle_span_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_endpoint_probe_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_hint_probe_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_canary_probe_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_fallback_probe_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_direct_probe_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_hint_candidate_px: usize,
    pub(in crate::gui::portmaster) solid_triangle_narrowed_rows: usize,
    pub(in crate::gui::portmaster) solid_triangle_full_scan_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_calls: usize,
    pub(in crate::gui::portmaster) solid_fan_triangles: usize,
    pub(in crate::gui::portmaster) solid_fan_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_px: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_intersections: usize,
    pub(in crate::gui::portmaster) solid_fan_endpoint_probe_px: usize,
    pub(in crate::gui::portmaster) solid_fan_fallback_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_calls: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_edges: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_used_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_fallback_budget: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_fallback_non_finite: usize,
    pub(in crate::gui::portmaster) solid_fan_edge_precompute_old_solver_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_hits: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_misses: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_hit_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_hit_px: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_stored_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_rejected_too_many_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_resident_entries: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_resident_rows: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_total_evictions: usize,
    pub(in crate::gui::portmaster) solid_fan_span_cache_row_budget_evictions: usize,
    pub(in crate::gui::portmaster) textured_triangle_calls: usize,
    pub(in crate::gui::portmaster) textured_triangle_bbox_px: usize,
    pub(in crate::gui::portmaster) textured_triangle_covered_px: usize,
    pub(in crate::gui::portmaster) textured_triangle_candidate_px: usize,
    pub(in crate::gui::portmaster) textured_triangle_narrowed_rows: usize,
    pub(in crate::gui::portmaster) textured_triangle_full_scan_rows: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_calls: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_texel_calls: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_non_white_texel_calls: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_eligible_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_candidate_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_covered_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_texel_covered_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_rows: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_match_rows:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_mismatch_rows:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_empty_rows:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_span_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_probe_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_render_rows:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_endpoint_render_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_scan_runs: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_scan_multi_run_rows:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_non_white_texel_covered_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_opaque_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_translucent_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_transparent_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_constant_alpha_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_constant_alpha_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_constant_color_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_constant_color_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_variable_color_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_variable_color_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_variable_alpha_run_calls:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_variable_alpha_run_px:
        usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_us: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_white_texel_us: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_non_white_texel_us: usize,
    pub(in crate::gui::portmaster) sampled_textured_triangle_calls: usize,
    pub(in crate::gui::portmaster) sampled_textured_triangle_candidate_px: usize,
    pub(in crate::gui::portmaster) sampled_textured_triangle_covered_px: usize,
    pub(in crate::gui::portmaster) sampled_textured_triangle_us: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_run_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_run_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_opaque_run_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_opaque_run_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_translucent_run_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_translucent_run_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_transparent_run_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_transparent_run_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_white_vertex_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_white_vertex_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_modulated_vertex_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_modulated_vertex_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_direct_calls: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_direct_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_direct_opaque_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_direct_translucent_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_direct_transparent_px: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_candidate_px: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_white_vertex_contiguous_px: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vertex_contiguous_px: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_runs: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_lt4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_4_7: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_8_15: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_16_31: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_32_63: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_contiguous_px_64_plus: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_opportunity_blocks_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_opportunity_px_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_true_tail_px_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_opportunity_blocks_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_opportunity_px_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_true_tail_px_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_backend_available: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_attempt_blocks_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_success_blocks_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_fallback_blocks_16: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_attempt_blocks_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_success_blocks_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_modulated_vector_fallback_blocks_4: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_blocks: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_opaque_blocks: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_transparent_blocks: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_mixed_blocks: usize,
    pub(in crate::gui::portmaster) textured_rect_sampled_vector_tail_px: usize,
    pub(in crate::gui::portmaster) textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16:
        [usize; 6],
    pub(in crate::gui::portmaster) degenerate_triangle_skips: usize,
    pub(in crate::gui::portmaster) fully_clipped_triangle_skips: usize,
    pub(in crate::gui::portmaster) opaque_px: usize,
    pub(in crate::gui::portmaster) translucent_px: usize,
    pub(in crate::gui::portmaster) transparent_px: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_runs: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_px_lt4: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_px_4_7: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_px_8_15: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_px_16_31: usize,
    pub(in crate::gui::portmaster) constant_texel_textured_triangle_span_px_32_plus: usize,
}

impl Default for RasterStats {
    fn default() -> Self {
        // SAFETY: RasterStats contains only usize fields and fixed-size usize arrays, so an
        // all-zero bit pattern is valid for every field. The backend availability marker is the
        // only counter that intentionally starts with a target-specific non-zero value.
        let mut stats: Self = unsafe { std::mem::zeroed() };
        stats.textured_rect_sampled_vector_backend_available =
            TEXTURED_RECT_SAMPLED_VECTOR_BACKEND_AVAILABLE;
        stats
    }
}

macro_rules! raster_stats_values {
    ($stats:expr) => {
        [
            $stats.solid_rect_calls,
            $stats.solid_rect_px,
            $stats.textured_rect_calls,
            $stats.textured_rect_px,
            $stats.textured_rect_constant_texel_calls,
            $stats.textured_rect_constant_texel_px,
            $stats.textured_rect_constant_texel_us,
            $stats.textured_rect_sampled_calls,
            $stats.textured_rect_sampled_px,
            $stats.textured_rect_sampled_us,
            $stats.textured_rect_separable_uv_calls,
            $stats.textured_rect_separable_uv_px,
            $stats.textured_rect_nonseparable_uv_calls,
            $stats.textured_rect_nonseparable_uv_px,
            $stats.textured_rect_white_texel_calls,
            $stats.textured_rect_white_texel_px,
            $stats.textured_rect_uniform_color_calls,
            $stats.textured_rect_uniform_color_px,
            $stats.solid_triangle_calls,
            $stats.solid_triangle_bbox_px,
            $stats.solid_triangle_covered_px,
            $stats.solid_triangle_span_rows,
            $stats.solid_triangle_candidate_px,
            $stats.solid_triangle_hint_rows,
            $stats.solid_triangle_hint_fallback_rows,
            $stats.solid_triangle_hint_build_us,
            $stats.solid_triangle_endpoint_search_us,
            $stats.solid_triangle_blend_span_us,
            $stats.solid_triangle_blend_span_calls,
            $stats.solid_triangle_span_px,
            $stats.solid_triangle_endpoint_probe_px,
            $stats.solid_triangle_hint_probe_px,
            $stats.solid_triangle_canary_probe_px,
            $stats.solid_triangle_fallback_probe_px,
            $stats.solid_triangle_direct_probe_px,
            $stats.solid_triangle_hint_candidate_px,
            $stats.solid_triangle_narrowed_rows,
            $stats.solid_triangle_full_scan_rows,
            $stats.solid_fan_calls,
            $stats.solid_fan_triangles,
            $stats.solid_fan_rows,
            $stats.solid_fan_px,
            $stats.solid_fan_edge_intersections,
            $stats.solid_fan_endpoint_probe_px,
            $stats.solid_fan_fallback_rows,
            $stats.solid_fan_edge_precompute_calls,
            $stats.solid_fan_edge_precompute_edges,
            $stats.solid_fan_edge_precompute_used_rows,
            $stats.solid_fan_edge_precompute_fallback_budget,
            $stats.solid_fan_edge_precompute_fallback_non_finite,
            $stats.solid_fan_edge_precompute_old_solver_rows,
            $stats.solid_fan_span_cache_hits,
            $stats.solid_fan_span_cache_misses,
            $stats.solid_fan_span_cache_hit_rows,
            $stats.solid_fan_span_cache_hit_px,
            $stats.solid_fan_span_cache_stored_rows,
            $stats.solid_fan_span_cache_rejected_too_many_rows,
            $stats.solid_fan_span_cache_resident_entries,
            $stats.solid_fan_span_cache_resident_rows,
            $stats.solid_fan_span_cache_total_evictions,
            $stats.solid_fan_span_cache_row_budget_evictions,
            $stats.textured_triangle_calls,
            $stats.textured_triangle_bbox_px,
            $stats.textured_triangle_covered_px,
            $stats.textured_triangle_candidate_px,
            $stats.textured_triangle_narrowed_rows,
            $stats.textured_triangle_full_scan_rows,
            $stats.constant_texel_textured_triangle_calls,
            $stats.constant_texel_textured_triangle_white_texel_calls,
            $stats.constant_texel_textured_triangle_non_white_texel_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_eligible_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls,
            $stats.constant_texel_textured_triangle_candidate_px,
            $stats.constant_texel_textured_triangle_covered_px,
            $stats.constant_texel_textured_triangle_white_texel_covered_px,
            $stats.constant_texel_textured_triangle_white_endpoint_rows,
            $stats.constant_texel_textured_triangle_white_endpoint_match_rows,
            $stats.constant_texel_textured_triangle_white_endpoint_mismatch_rows,
            $stats.constant_texel_textured_triangle_white_endpoint_empty_rows,
            $stats.constant_texel_textured_triangle_white_endpoint_span_px,
            $stats.constant_texel_textured_triangle_white_endpoint_probe_px,
            $stats.constant_texel_textured_triangle_white_endpoint_render_rows,
            $stats.constant_texel_textured_triangle_white_endpoint_render_px,
            $stats.constant_texel_textured_triangle_white_scan_runs,
            $stats.constant_texel_textured_triangle_white_scan_multi_run_rows,
            $stats.constant_texel_textured_triangle_non_white_texel_covered_px,
            $stats.constant_texel_textured_triangle_opaque_px,
            $stats.constant_texel_textured_triangle_translucent_px,
            $stats.constant_texel_textured_triangle_transparent_px,
            $stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px,
            $stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls,
            $stats.constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px,
            $stats.constant_texel_textured_triangle_white_constant_alpha_run_calls,
            $stats.constant_texel_textured_triangle_white_constant_alpha_run_px,
            $stats.constant_texel_textured_triangle_white_constant_color_run_calls,
            $stats.constant_texel_textured_triangle_white_constant_color_run_px,
            $stats.constant_texel_textured_triangle_white_variable_color_run_calls,
            $stats.constant_texel_textured_triangle_white_variable_color_run_px,
            $stats.constant_texel_textured_triangle_white_variable_alpha_run_calls,
            $stats.constant_texel_textured_triangle_white_variable_alpha_run_px,
            $stats.constant_texel_textured_triangle_us,
            $stats.constant_texel_textured_triangle_white_texel_us,
            $stats.constant_texel_textured_triangle_non_white_texel_us,
            $stats.sampled_textured_triangle_calls,
            $stats.sampled_textured_triangle_candidate_px,
            $stats.sampled_textured_triangle_covered_px,
            $stats.sampled_textured_triangle_us,
            $stats.textured_rect_separable_run_calls,
            $stats.textured_rect_separable_run_px,
            $stats.textured_rect_separable_opaque_run_calls,
            $stats.textured_rect_separable_opaque_run_px,
            $stats.textured_rect_separable_translucent_run_calls,
            $stats.textured_rect_separable_translucent_run_px,
            $stats.textured_rect_separable_transparent_run_calls,
            $stats.textured_rect_separable_transparent_run_px,
            $stats.textured_rect_separable_white_vertex_calls,
            $stats.textured_rect_separable_white_vertex_px,
            $stats.textured_rect_separable_modulated_vertex_calls,
            $stats.textured_rect_separable_modulated_vertex_px,
            $stats.textured_rect_separable_direct_calls,
            $stats.textured_rect_separable_direct_px,
            $stats.textured_rect_separable_direct_opaque_px,
            $stats.textured_rect_separable_direct_translucent_px,
            $stats.textured_rect_separable_direct_transparent_px,
            $stats.textured_rect_sampled_vector_candidate_px,
            $stats.textured_rect_sampled_white_vertex_contiguous_px,
            $stats.textured_rect_sampled_modulated_vertex_contiguous_px,
            $stats.textured_rect_sampled_modulated_contiguous_runs,
            $stats.textured_rect_sampled_modulated_contiguous_px_lt4,
            $stats.textured_rect_sampled_modulated_contiguous_px_4_7,
            $stats.textured_rect_sampled_modulated_contiguous_px_8_15,
            $stats.textured_rect_sampled_modulated_contiguous_px_16_31,
            $stats.textured_rect_sampled_modulated_contiguous_px_32_63,
            $stats.textured_rect_sampled_modulated_contiguous_px_64_plus,
            $stats.textured_rect_sampled_modulated_opportunity_blocks_16,
            $stats.textured_rect_sampled_modulated_opportunity_px_16,
            $stats.textured_rect_sampled_modulated_true_tail_px_16,
            $stats.textured_rect_sampled_modulated_opportunity_blocks_4,
            $stats.textured_rect_sampled_modulated_opportunity_px_4,
            $stats.textured_rect_sampled_modulated_true_tail_px_4,
            $stats.textured_rect_sampled_vector_backend_available,
            $stats.textured_rect_sampled_modulated_vector_attempt_blocks_16,
            $stats.textured_rect_sampled_modulated_vector_success_blocks_16,
            $stats.textured_rect_sampled_modulated_vector_fallback_blocks_16,
            $stats.textured_rect_sampled_modulated_vector_attempt_blocks_4,
            $stats.textured_rect_sampled_modulated_vector_success_blocks_4,
            $stats.textured_rect_sampled_modulated_vector_fallback_blocks_4,
            $stats.textured_rect_sampled_vector_blocks,
            $stats.textured_rect_sampled_vector_opaque_blocks,
            $stats.textured_rect_sampled_vector_transparent_blocks,
            $stats.textured_rect_sampled_vector_mixed_blocks,
            $stats.textured_rect_sampled_vector_tail_px,
            $stats.degenerate_triangle_skips,
            $stats.fully_clipped_triangle_skips,
            $stats.opaque_px,
            $stats.translucent_px,
            $stats.transparent_px,
            $stats.constant_texel_textured_triangle_span_runs,
            $stats.constant_texel_textured_triangle_span_px_lt4,
            $stats.constant_texel_textured_triangle_span_px_4_7,
            $stats.constant_texel_textured_triangle_span_px_8_15,
            $stats.constant_texel_textured_triangle_span_px_16_31,
            $stats.constant_texel_textured_triangle_span_px_32_plus,
        ]
    };
}

impl RasterStats {
    pub(super) fn record_textured_rect_sampled_vector_candidate(&mut self, white: bool, px: usize) {
        self.textured_rect_sampled_vector_candidate_px += px;
        if white {
            self.textured_rect_sampled_white_vertex_contiguous_px += px;
        } else {
            self.textured_rect_sampled_modulated_vertex_contiguous_px += px;
            self.record_textured_rect_sampled_modulated_contiguous_run(px);
            self.record_textured_rect_sampled_modulated_opportunity_16(px);
            self.record_textured_rect_sampled_modulated_opportunity_4(px);
        }
    }

    fn record_textured_rect_sampled_modulated_opportunity_16(&mut self, px: usize) {
        let blocks = px / TEXTURED_RECT_VECTOR_BLOCK_PX;
        let opportunity_px = blocks * TEXTURED_RECT_VECTOR_BLOCK_PX;
        self.textured_rect_sampled_modulated_opportunity_blocks_16 += blocks;
        self.textured_rect_sampled_modulated_opportunity_px_16 += opportunity_px;
        self.textured_rect_sampled_modulated_true_tail_px_16 += px - opportunity_px;
    }

    fn record_textured_rect_sampled_modulated_opportunity_4(&mut self, px: usize) {
        let remainder_after_16 = px % TEXTURED_RECT_VECTOR_BLOCK_PX;
        let blocks = remainder_after_16 / 4;
        let opportunity_px = blocks * 4;
        self.textured_rect_sampled_modulated_opportunity_blocks_4 += blocks;
        self.textured_rect_sampled_modulated_opportunity_px_4 += opportunity_px;
        self.textured_rect_sampled_modulated_true_tail_px_4 += remainder_after_16 - opportunity_px;
    }

    fn record_textured_rect_sampled_modulated_contiguous_run(&mut self, px: usize) {
        self.textured_rect_sampled_modulated_contiguous_runs += 1;
        match px {
            0..=3 => self.textured_rect_sampled_modulated_contiguous_px_lt4 += px,
            4..=7 => self.textured_rect_sampled_modulated_contiguous_px_4_7 += px,
            8..=15 => self.textured_rect_sampled_modulated_contiguous_px_8_15 += px,
            16..=31 => self.textured_rect_sampled_modulated_contiguous_px_16_31 += px,
            32..=63 => self.textured_rect_sampled_modulated_contiguous_px_32_63 += px,
            _ => self.textured_rect_sampled_modulated_contiguous_px_64_plus += px,
        }
    }

    pub(super) fn record_textured_rect_sampled_vector_block(
        &mut self,
        block_px: usize,
        opaque_px: usize,
        transparent_px: usize,
    ) {
        self.textured_rect_sampled_vector_blocks += 1;
        match (opaque_px, transparent_px) {
            (opaque_px, _) if opaque_px == block_px => {
                self.textured_rect_sampled_vector_opaque_blocks += 1;
            }
            (_, transparent_px) if transparent_px == block_px => {
                self.textured_rect_sampled_vector_transparent_blocks += 1;
            }
            _ => self.textured_rect_sampled_vector_mixed_blocks += 1,
        }
    }

    pub(super) fn record_textured_rect_sampled_modulated_vector_attempt(&mut self, success: bool) {
        self.textured_rect_sampled_modulated_vector_attempt_blocks_16 += 1;
        if success {
            self.textured_rect_sampled_modulated_vector_success_blocks_16 += 1;
        } else {
            self.textured_rect_sampled_modulated_vector_fallback_blocks_16 += 1;
        }
    }

    pub(super) fn record_textured_rect_sampled_modulated_vector_attempt_4(
        &mut self,
        success: bool,
    ) {
        self.textured_rect_sampled_modulated_vector_attempt_blocks_4 += 1;
        if success {
            self.textured_rect_sampled_modulated_vector_success_blocks_4 += 1;
        } else {
            self.textured_rect_sampled_modulated_vector_fallback_blocks_4 += 1;
        }
    }

    pub(super) fn record_alpha_px(&mut self, alpha: u8, count: usize) {
        match alpha {
            0 => self.transparent_px += count,
            u8::MAX => self.opaque_px += count,
            _ => self.translucent_px += count,
        }
    }

    pub(super) fn record_constant_texel_alpha_px(&mut self, alpha: u8, count: usize) {
        match alpha {
            0 => self.constant_texel_textured_triangle_transparent_px += count,
            u8::MAX => self.constant_texel_textured_triangle_opaque_px += count,
            _ => self.constant_texel_textured_triangle_translucent_px += count,
        }
    }

    pub(super) fn record_constant_texel_textured_triangle_span_run(&mut self, px: usize) {
        if px == 0 {
            return;
        }

        self.constant_texel_textured_triangle_span_runs += 1;
        match px {
            1..=3 => self.constant_texel_textured_triangle_span_px_lt4 += px,
            4..=7 => self.constant_texel_textured_triangle_span_px_4_7 += px,
            8..=15 => self.constant_texel_textured_triangle_span_px_8_15 += px,
            16..=31 => self.constant_texel_textured_triangle_span_px_16_31 += px,
            _ => self.constant_texel_textured_triangle_span_px_32_plus += px,
        }
    }

    pub(super) fn record_textured_rect_separable_vertex_color(&mut self, white: bool, px: usize) {
        if white {
            self.textured_rect_separable_white_vertex_calls += 1;
            self.textured_rect_separable_white_vertex_px += px;
        } else {
            self.textured_rect_separable_modulated_vertex_calls += 1;
            self.textured_rect_separable_modulated_vertex_px += px;
        }
    }

    pub(super) fn record_textured_rect_separable_direct_alpha_px(
        &mut self,
        alpha: u8,
        count: usize,
    ) {
        match alpha {
            0 => self.textured_rect_separable_direct_transparent_px += count,
            u8::MAX => self.textured_rect_separable_direct_opaque_px += count,
            _ => self.textured_rect_separable_direct_translucent_px += count,
        }
    }

    pub(in crate::gui::portmaster) fn log_line(&self) -> String {
        use std::fmt::Write as _;

        let values = raster_stats_values!(self);
        let mut line = String::new();
        write!(&mut line, "software renderer raster_stats solid_rect_calls={} solid_rect_px={} textured_rect_calls={} textured_rect_px={} textured_rect_constant_texel_calls={} textured_rect_constant_texel_px={} textured_rect_constant_texel_us={} textured_rect_sampled_calls={} textured_rect_sampled_px={} textured_rect_sampled_us={} textured_rect_separable_uv_calls={} textured_rect_separable_uv_px={} textured_rect_nonseparable_uv_calls={} textured_rect_nonseparable_uv_px={} textured_rect_white_texel_calls={} textured_rect_white_texel_px={} textured_rect_uniform_color_calls={} textured_rect_uniform_color_px={} solid_triangle_calls={} solid_triangle_bbox_px={} solid_triangle_covered_px={} solid_triangle_span_rows={} solid_triangle_candidate_px={} solid_triangle_hint_rows={}", values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7], values[8], values[9], values[10], values[11], values[12], values[13], values[14], values[15], values[16], values[17], values[18], values[19], values[20], values[21], values[22], values[23]).expect("writing to a String cannot fail");
        write!(&mut line, " solid_triangle_hint_fallback_rows={} solid_triangle_hint_build_us={} solid_triangle_endpoint_search_us={} solid_triangle_blend_span_us={} solid_triangle_blend_span_calls={} solid_triangle_span_px={} solid_triangle_endpoint_probe_px={} solid_triangle_hint_probe_px={} solid_triangle_canary_probe_px={} solid_triangle_fallback_probe_px={} solid_triangle_direct_probe_px={} solid_triangle_hint_candidate_px={} solid_triangle_narrowed_rows={} solid_triangle_full_scan_rows={} solid_fan_calls={} solid_fan_triangles={} solid_fan_rows={} solid_fan_px={} solid_fan_edge_intersections={} solid_fan_endpoint_probe_px={}", values[24], values[25], values[26], values[27], values[28], values[29], values[30], values[31], values[32], values[33], values[34], values[35], values[36], values[37], values[38], values[39], values[40], values[41], values[42], values[43]).expect("writing to a String cannot fail");
        write!(&mut line, " solid_fan_fallback_rows={} solid_fan_edge_precompute_calls={} solid_fan_edge_precompute_edges={} solid_fan_edge_precompute_used_rows={} solid_fan_edge_precompute_fallback_budget={} solid_fan_edge_precompute_fallback_non_finite={} solid_fan_edge_precompute_old_solver_rows={} solid_fan_span_cache_hits={} solid_fan_span_cache_misses={} solid_fan_span_cache_hit_rows={} solid_fan_span_cache_hit_px={} solid_fan_span_cache_stored_rows={} solid_fan_span_cache_rejected_too_many_rows={} solid_fan_span_cache_resident_entries={} solid_fan_span_cache_resident_rows={} solid_fan_span_cache_total_evictions={} solid_fan_span_cache_row_budget_evictions={} textured_triangle_calls={} textured_triangle_bbox_px={} textured_triangle_covered_px={}", values[44], values[45], values[46], values[47], values[48], values[49], values[50], values[51], values[52], values[53], values[54], values[55], values[56], values[57], values[58], values[59], values[60], values[61], values[62], values[63]).expect("writing to a String cannot fail");
        write!(&mut line, " textured_triangle_candidate_px={} textured_triangle_narrowed_rows={} textured_triangle_full_scan_rows={} constant_texel_textured_triangle_calls={} constant_texel_textured_triangle_white_texel_calls={} constant_texel_textured_triangle_non_white_texel_calls={} constant_texel_textured_triangle_white_alpha_only_eligible_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls={} constant_texel_textured_triangle_candidate_px={} constant_texel_textured_triangle_covered_px={} constant_texel_textured_triangle_white_texel_covered_px={} constant_texel_textured_triangle_white_endpoint_rows={} constant_texel_textured_triangle_white_endpoint_match_rows={} constant_texel_textured_triangle_white_endpoint_mismatch_rows={} constant_texel_textured_triangle_white_endpoint_empty_rows={} constant_texel_textured_triangle_white_endpoint_span_px={} constant_texel_textured_triangle_white_endpoint_probe_px={} constant_texel_textured_triangle_white_endpoint_render_rows={} constant_texel_textured_triangle_white_endpoint_render_px={} constant_texel_textured_triangle_white_scan_runs={}", values[64], values[65], values[66], values[67], values[68], values[69], values[70], values[71], values[72], values[73], values[74], values[75], values[76], values[77], values[78], values[79], values[80], values[81], values[82], values[83], values[84], values[85]).expect("writing to a String cannot fail");
        write!(&mut line, " constant_texel_textured_triangle_white_scan_multi_run_rows={} constant_texel_textured_triangle_non_white_texel_covered_px={} constant_texel_textured_triangle_opaque_px={} constant_texel_textured_triangle_translucent_px={} constant_texel_textured_triangle_transparent_px={} constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls={} constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px={} constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls={} constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px={} constant_texel_textured_triangle_white_constant_alpha_run_calls={} constant_texel_textured_triangle_white_constant_alpha_run_px={} constant_texel_textured_triangle_white_constant_color_run_calls={} constant_texel_textured_triangle_white_constant_color_run_px={} constant_texel_textured_triangle_white_variable_color_run_calls={} constant_texel_textured_triangle_white_variable_color_run_px={} constant_texel_textured_triangle_white_variable_alpha_run_calls={} constant_texel_textured_triangle_white_variable_alpha_run_px={} constant_texel_textured_triangle_us={} constant_texel_textured_triangle_white_texel_us={} constant_texel_textured_triangle_non_white_texel_us={}", values[86], values[87], values[88], values[89], values[90], values[91], values[92], values[93], values[94], values[95], values[96], values[97], values[98], values[99], values[100], values[101], values[102], values[103], values[104], values[105]).expect("writing to a String cannot fail");
        write!(&mut line, " sampled_textured_triangle_calls={} sampled_textured_triangle_candidate_px={} sampled_textured_triangle_covered_px={} sampled_textured_triangle_us={} textured_rect_separable_run_calls={} textured_rect_separable_run_px={} textured_rect_separable_opaque_run_calls={} textured_rect_separable_opaque_run_px={} textured_rect_separable_translucent_run_calls={} textured_rect_separable_translucent_run_px={} textured_rect_separable_transparent_run_calls={} textured_rect_separable_transparent_run_px={} textured_rect_separable_white_vertex_calls={} textured_rect_separable_white_vertex_px={} textured_rect_separable_modulated_vertex_calls={} textured_rect_separable_modulated_vertex_px={} textured_rect_separable_direct_calls={} textured_rect_separable_direct_px={} textured_rect_separable_direct_opaque_px={} textured_rect_separable_direct_translucent_px={} textured_rect_separable_direct_transparent_px={} textured_rect_sampled_vector_candidate_px={} textured_rect_sampled_white_vertex_contiguous_px={} textured_rect_sampled_modulated_vertex_contiguous_px={} textured_rect_sampled_modulated_contiguous_runs={} textured_rect_sampled_modulated_contiguous_px_lt4={} textured_rect_sampled_modulated_contiguous_px_4_7={} textured_rect_sampled_modulated_contiguous_px_8_15={} textured_rect_sampled_modulated_contiguous_px_16_31={} textured_rect_sampled_modulated_contiguous_px_32_63={} textured_rect_sampled_modulated_contiguous_px_64_plus={} textured_rect_sampled_modulated_opportunity_blocks_16={} textured_rect_sampled_modulated_opportunity_px_16={} textured_rect_sampled_modulated_true_tail_px_16={} textured_rect_sampled_modulated_opportunity_blocks_4={} textured_rect_sampled_modulated_opportunity_px_4={} textured_rect_sampled_modulated_true_tail_px_4={} textured_rect_sampled_vector_backend_available={} textured_rect_sampled_modulated_vector_attempt_blocks_16={} textured_rect_sampled_modulated_vector_success_blocks_16={} textured_rect_sampled_modulated_vector_fallback_blocks_16={} textured_rect_sampled_modulated_vector_attempt_blocks_4={} textured_rect_sampled_modulated_vector_success_blocks_4={} textured_rect_sampled_modulated_vector_fallback_blocks_4={} textured_rect_sampled_vector_blocks={} textured_rect_sampled_vector_opaque_blocks={} textured_rect_sampled_vector_transparent_blocks={} textured_rect_sampled_vector_mixed_blocks={} textured_rect_sampled_vector_tail_px={}", values[106], values[107], values[108], values[109], values[110], values[111], values[112], values[113], values[114], values[115], values[116], values[117], values[118], values[119], values[120], values[121], values[122], values[123], values[124], values[125], values[126], values[127], values[128], values[129], values[130], values[131], values[132], values[133], values[134], values[135], values[136], values[137], values[138], values[139], values[140], values[141], values[142], values[143], values[144], values[145], values[146], values[147], values[148], values[149], values[150], values[151], values[152], values[153], values[154]).expect("writing to a String cannot fail");
        let buckets = self.textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16;
        write!(&mut line, " textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16={},{},{},{},{},{} degenerate_triangle_skips={} fully_clipped_triangle_skips={} opaque_px={} translucent_px={} transparent_px={} constant_texel_textured_triangle_span_runs={} constant_texel_textured_triangle_span_px_lt4={} constant_texel_textured_triangle_span_px_4_7={} constant_texel_textured_triangle_span_px_8_15={} constant_texel_textured_triangle_span_px_16_31={} constant_texel_textured_triangle_span_px_32_plus={}", buckets[0], buckets[1], buckets[2], buckets[3], buckets[4], buckets[5], values[155], values[156], values[157], values[158], values[159], values[160], values[161], values[162], values[163], values[164], values[165]).expect("writing to a String cannot fail");
        line
    }
}

#[cfg(test)]
mod tests {
    use super::{RasterStats, TEXTURED_RECT_SAMPLED_VECTOR_BACKEND_AVAILABLE};

    macro_rules! populated_raster_stats {
        () => {
            RasterStats {
                solid_rect_calls: 1,
                solid_rect_px: 2,
                textured_rect_calls: 3,
                textured_rect_px: 4,
                textured_rect_constant_texel_calls: 5,
                textured_rect_constant_texel_px: 6,
                textured_rect_constant_texel_us: 7,
                textured_rect_sampled_calls: 8,
                textured_rect_sampled_px: 9,
                textured_rect_sampled_us: 10,
                textured_rect_separable_uv_calls: 11,
                textured_rect_separable_uv_px: 12,
                textured_rect_nonseparable_uv_calls: 13,
                textured_rect_nonseparable_uv_px: 14,
                textured_rect_white_texel_calls: 15,
                textured_rect_white_texel_px: 16,
                textured_rect_uniform_color_calls: 17,
                textured_rect_uniform_color_px: 18,
                solid_triangle_calls: 19,
                solid_triangle_bbox_px: 20,
                solid_triangle_covered_px: 21,
                solid_triangle_span_rows: 22,
                solid_triangle_candidate_px: 23,
                solid_triangle_hint_rows: 24,
                solid_triangle_hint_fallback_rows: 25,
                solid_triangle_hint_build_us: 26,
                solid_triangle_endpoint_search_us: 27,
                solid_triangle_blend_span_us: 28,
                solid_triangle_blend_span_calls: 29,
                solid_triangle_span_px: 30,
                solid_triangle_endpoint_probe_px: 31,
                solid_triangle_hint_probe_px: 32,
                solid_triangle_canary_probe_px: 33,
                solid_triangle_fallback_probe_px: 34,
                solid_triangle_direct_probe_px: 35,
                solid_triangle_hint_candidate_px: 36,
                solid_triangle_narrowed_rows: 37,
                solid_triangle_full_scan_rows: 38,
                solid_fan_calls: 39,
                solid_fan_triangles: 40,
                solid_fan_rows: 41,
                solid_fan_px: 42,
                solid_fan_edge_intersections: 43,
                solid_fan_endpoint_probe_px: 44,
                solid_fan_fallback_rows: 45,
                solid_fan_edge_precompute_calls: 46,
                solid_fan_edge_precompute_edges: 47,
                solid_fan_edge_precompute_used_rows: 48,
                solid_fan_edge_precompute_fallback_budget: 49,
                solid_fan_edge_precompute_fallback_non_finite: 50,
                solid_fan_edge_precompute_old_solver_rows: 51,
                solid_fan_span_cache_hits: 52,
                solid_fan_span_cache_misses: 53,
                solid_fan_span_cache_hit_rows: 54,
                solid_fan_span_cache_hit_px: 55,
                solid_fan_span_cache_stored_rows: 56,
                solid_fan_span_cache_rejected_too_many_rows: 57,
                solid_fan_span_cache_resident_entries: 58,
                solid_fan_span_cache_resident_rows: 59,
                solid_fan_span_cache_total_evictions: 60,
                solid_fan_span_cache_row_budget_evictions: 61,
                textured_triangle_calls: 62,
                textured_triangle_bbox_px: 63,
                textured_triangle_covered_px: 64,
                textured_triangle_candidate_px: 65,
                textured_triangle_narrowed_rows: 66,
                textured_triangle_full_scan_rows: 67,
                constant_texel_textured_triangle_calls: 68,
                constant_texel_textured_triangle_white_texel_calls: 69,
                constant_texel_textured_triangle_non_white_texel_calls: 70,
                constant_texel_textured_triangle_white_alpha_only_eligible_calls: 71,
                constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls: 72,
                constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls: 73,
                constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls: 74,
                constant_texel_textured_triangle_candidate_px: 75,
                constant_texel_textured_triangle_covered_px: 76,
                constant_texel_textured_triangle_white_texel_covered_px: 77,
                constant_texel_textured_triangle_white_endpoint_rows: 78,
                constant_texel_textured_triangle_white_endpoint_match_rows: 79,
                constant_texel_textured_triangle_white_endpoint_mismatch_rows: 80,
                constant_texel_textured_triangle_white_endpoint_empty_rows: 81,
                constant_texel_textured_triangle_white_endpoint_span_px: 82,
                constant_texel_textured_triangle_white_endpoint_probe_px: 83,
                constant_texel_textured_triangle_white_endpoint_render_rows: 84,
                constant_texel_textured_triangle_white_endpoint_render_px: 85,
                constant_texel_textured_triangle_white_scan_runs: 86,
                constant_texel_textured_triangle_white_scan_multi_run_rows: 87,
                constant_texel_textured_triangle_non_white_texel_covered_px: 88,
                constant_texel_textured_triangle_opaque_px: 89,
                constant_texel_textured_triangle_translucent_px: 90,
                constant_texel_textured_triangle_transparent_px: 91,
                constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls: 92,
                constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px: 93,
                constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls: 94,
                constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px: 95,
                constant_texel_textured_triangle_white_constant_alpha_run_calls: 96,
                constant_texel_textured_triangle_white_constant_alpha_run_px: 97,
                constant_texel_textured_triangle_white_constant_color_run_calls: 98,
                constant_texel_textured_triangle_white_constant_color_run_px: 99,
                constant_texel_textured_triangle_white_variable_color_run_calls: 100,
                constant_texel_textured_triangle_white_variable_color_run_px: 101,
                constant_texel_textured_triangle_white_variable_alpha_run_calls: 102,
                constant_texel_textured_triangle_white_variable_alpha_run_px: 103,
                constant_texel_textured_triangle_us: 104,
                constant_texel_textured_triangle_white_texel_us: 105,
                constant_texel_textured_triangle_non_white_texel_us: 106,
                sampled_textured_triangle_calls: 107,
                sampled_textured_triangle_candidate_px: 108,
                sampled_textured_triangle_covered_px: 109,
                sampled_textured_triangle_us: 110,
                textured_rect_separable_run_calls: 111,
                textured_rect_separable_run_px: 112,
                textured_rect_separable_opaque_run_calls: 113,
                textured_rect_separable_opaque_run_px: 114,
                textured_rect_separable_translucent_run_calls: 115,
                textured_rect_separable_translucent_run_px: 116,
                textured_rect_separable_transparent_run_calls: 117,
                textured_rect_separable_transparent_run_px: 118,
                textured_rect_separable_white_vertex_calls: 119,
                textured_rect_separable_white_vertex_px: 120,
                textured_rect_separable_modulated_vertex_calls: 121,
                textured_rect_separable_modulated_vertex_px: 122,
                textured_rect_separable_direct_calls: 123,
                textured_rect_separable_direct_px: 124,
                textured_rect_separable_direct_opaque_px: 125,
                textured_rect_separable_direct_translucent_px: 126,
                textured_rect_separable_direct_transparent_px: 127,
                textured_rect_sampled_vector_candidate_px: 128,
                textured_rect_sampled_white_vertex_contiguous_px: 129,
                textured_rect_sampled_modulated_vertex_contiguous_px: 130,
                textured_rect_sampled_modulated_contiguous_runs: 131,
                textured_rect_sampled_modulated_contiguous_px_lt4: 132,
                textured_rect_sampled_modulated_contiguous_px_4_7: 133,
                textured_rect_sampled_modulated_contiguous_px_8_15: 134,
                textured_rect_sampled_modulated_contiguous_px_16_31: 135,
                textured_rect_sampled_modulated_contiguous_px_32_63: 136,
                textured_rect_sampled_modulated_contiguous_px_64_plus: 137,
                textured_rect_sampled_modulated_opportunity_blocks_16: 138,
                textured_rect_sampled_modulated_opportunity_px_16: 139,
                textured_rect_sampled_modulated_true_tail_px_16: 140,
                textured_rect_sampled_modulated_opportunity_blocks_4: 141,
                textured_rect_sampled_modulated_opportunity_px_4: 142,
                textured_rect_sampled_modulated_true_tail_px_4: 143,
                textured_rect_sampled_vector_backend_available: 144,
                textured_rect_sampled_modulated_vector_attempt_blocks_16: 145,
                textured_rect_sampled_modulated_vector_success_blocks_16: 146,
                textured_rect_sampled_modulated_vector_fallback_blocks_16: 147,
                textured_rect_sampled_modulated_vector_attempt_blocks_4: 148,
                textured_rect_sampled_modulated_vector_success_blocks_4: 149,
                textured_rect_sampled_modulated_vector_fallback_blocks_4: 150,
                textured_rect_sampled_vector_blocks: 151,
                textured_rect_sampled_vector_opaque_blocks: 152,
                textured_rect_sampled_vector_transparent_blocks: 153,
                textured_rect_sampled_vector_mixed_blocks: 154,
                textured_rect_sampled_vector_tail_px: 155,
                textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16: [
                    156, 157, 158, 159, 160, 161,
                ],
                degenerate_triangle_skips: 162,
                fully_clipped_triangle_skips: 163,
                opaque_px: 164,
                translucent_px: 165,
                transparent_px: 166,
                constant_texel_textured_triangle_span_runs: 167,
                constant_texel_textured_triangle_span_px_lt4: 168,
                constant_texel_textured_triangle_span_px_4_7: 169,
                constant_texel_textured_triangle_span_px_8_15: 170,
                constant_texel_textured_triangle_span_px_16_31: 171,
                constant_texel_textured_triangle_span_px_32_plus: 172,
            }
        };
    }

    #[test]
    fn raster_stats_log_line_includes_all_counters() {
        let stats = populated_raster_stats!();

        let line = stats.log_line();
        let actual_values: Vec<_> = line
            .split_whitespace()
            .skip(3)
            .filter_map(|entry| {
                entry
                    .split_once('=')
                    .expect("stats entry has separator")
                    .1
                    .parse::<usize>()
                    .ok()
            })
            .collect();

        assert!(line.starts_with("software renderer raster_stats solid_rect_calls=1 "));
        assert!(line.contains(" textured_rect_separable_uv_calls=11 "));
        assert!(line.contains(" textured_rect_nonseparable_uv_px=14 "));
        assert!(line.contains(
            " textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16=156,157,158,159,160,161 "
        ));
        assert!(line.contains(" textured_rect_sampled_vector_candidate_px=128 "));
        assert!(line.contains(" textured_rect_sampled_modulated_contiguous_runs=131 "));
        assert!(line.contains(" textured_rect_sampled_modulated_contiguous_px_64_plus=137 "));
        assert!(line.contains(" textured_rect_sampled_modulated_opportunity_blocks_16=138 "));
        assert!(line.contains(" textured_rect_sampled_modulated_opportunity_px_16=139 "));
        assert!(line.contains(" textured_rect_sampled_modulated_true_tail_px_16=140 "));
        assert!(line.contains(" textured_rect_sampled_modulated_opportunity_blocks_4=141 "));
        assert!(line.contains(" textured_rect_sampled_modulated_opportunity_px_4=142 "));
        assert!(line.contains(" textured_rect_sampled_modulated_true_tail_px_4=143 "));
        assert!(line.contains(" textured_rect_sampled_vector_backend_available=144 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_attempt_blocks_16=145 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_success_blocks_16=146 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_fallback_blocks_16=147 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_attempt_blocks_4=148 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_success_blocks_4=149 "));
        assert!(line.contains(" textured_rect_sampled_modulated_vector_fallback_blocks_4=150 "));
        assert!(line.contains(" textured_rect_sampled_vector_tail_px=155 "));
        assert_eq!(
            actual_values,
            (1..=155).chain(162..=172).collect::<Vec<_>>()
        );
    }

    #[test]
    fn raster_stats_default_records_sampled_rect_vector_backend_marker() {
        let stats = RasterStats::default();
        let trait_stats = <RasterStats as Default>::default();

        assert_eq!(
            stats.textured_rect_sampled_vector_backend_available,
            TEXTURED_RECT_SAMPLED_VECTOR_BACKEND_AVAILABLE
        );
        assert_eq!(
            trait_stats.textured_rect_sampled_vector_backend_available,
            TEXTURED_RECT_SAMPLED_VECTOR_BACKEND_AVAILABLE
        );
        assert_eq!(
            stats.textured_rect_sampled_vector_backend_available,
            trait_stats.textured_rect_sampled_vector_backend_available
        );
        assert!(stats.textured_rect_sampled_vector_backend_available <= 1);

        let mut expected = stats;
        expected.textured_rect_sampled_vector_backend_available = 0;
        assert_eq!(raster_stats_values!(&expected), [0; 166]);
        assert_eq!(
            stats.textured_rect_separable_run_px_buckets_le1_le2_le4_le8_le16_gt16,
            [0; 6]
        );
    }
}
