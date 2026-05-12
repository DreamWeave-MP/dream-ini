// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
    pub(in crate::gui::portmaster) degenerate_triangle_skips: usize,
    pub(in crate::gui::portmaster) fully_clipped_triangle_skips: usize,
    pub(in crate::gui::portmaster) opaque_px: usize,
    pub(in crate::gui::portmaster) translucent_px: usize,
    pub(in crate::gui::portmaster) transparent_px: usize,
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
            $stats.degenerate_triangle_skips,
            $stats.fully_clipped_triangle_skips,
            $stats.opaque_px,
            $stats.translucent_px,
            $stats.transparent_px,
        ]
    };
}

impl RasterStats {
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

    pub(in crate::gui::portmaster) fn log_line(&self) -> String {
        use std::fmt::Write as _;

        let values = raster_stats_values!(self);
        let mut line = String::new();
        write!(&mut line, "software renderer raster_stats solid_rect_calls={} solid_rect_px={} textured_rect_calls={} textured_rect_px={} textured_rect_constant_texel_calls={} textured_rect_constant_texel_px={} textured_rect_constant_texel_us={} textured_rect_sampled_calls={} textured_rect_sampled_px={} textured_rect_sampled_us={} textured_rect_separable_uv_calls={} textured_rect_separable_uv_px={} textured_rect_nonseparable_uv_calls={} textured_rect_nonseparable_uv_px={} textured_rect_white_texel_calls={} textured_rect_white_texel_px={} textured_rect_uniform_color_calls={} textured_rect_uniform_color_px={} solid_triangle_calls={} solid_triangle_bbox_px={} solid_triangle_covered_px={} solid_triangle_span_rows={} solid_triangle_candidate_px={} solid_triangle_hint_rows={}", values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7], values[8], values[9], values[10], values[11], values[12], values[13], values[14], values[15], values[16], values[17], values[18], values[19], values[20], values[21], values[22], values[23]).expect("writing to a String cannot fail");
        write!(&mut line, " solid_triangle_hint_fallback_rows={} solid_triangle_hint_build_us={} solid_triangle_endpoint_search_us={} solid_triangle_blend_span_us={} solid_triangle_blend_span_calls={} solid_triangle_span_px={} solid_triangle_endpoint_probe_px={} solid_triangle_hint_probe_px={} solid_triangle_canary_probe_px={} solid_triangle_fallback_probe_px={} solid_triangle_direct_probe_px={} solid_triangle_hint_candidate_px={} solid_triangle_narrowed_rows={} solid_triangle_full_scan_rows={} solid_fan_calls={} solid_fan_triangles={} solid_fan_rows={} solid_fan_px={} solid_fan_edge_intersections={} solid_fan_endpoint_probe_px={}", values[24], values[25], values[26], values[27], values[28], values[29], values[30], values[31], values[32], values[33], values[34], values[35], values[36], values[37], values[38], values[39], values[40], values[41], values[42], values[43]).expect("writing to a String cannot fail");
        write!(&mut line, " solid_fan_fallback_rows={} solid_fan_edge_precompute_calls={} solid_fan_edge_precompute_edges={} solid_fan_edge_precompute_used_rows={} solid_fan_edge_precompute_fallback_budget={} solid_fan_edge_precompute_fallback_non_finite={} solid_fan_edge_precompute_old_solver_rows={} solid_fan_span_cache_hits={} solid_fan_span_cache_misses={} solid_fan_span_cache_hit_rows={} solid_fan_span_cache_hit_px={} solid_fan_span_cache_stored_rows={} solid_fan_span_cache_rejected_too_many_rows={} solid_fan_span_cache_resident_entries={} solid_fan_span_cache_resident_rows={} solid_fan_span_cache_total_evictions={} solid_fan_span_cache_row_budget_evictions={} textured_triangle_calls={} textured_triangle_bbox_px={} textured_triangle_covered_px={}", values[44], values[45], values[46], values[47], values[48], values[49], values[50], values[51], values[52], values[53], values[54], values[55], values[56], values[57], values[58], values[59], values[60], values[61], values[62], values[63]).expect("writing to a String cannot fail");
        write!(&mut line, " textured_triangle_candidate_px={} textured_triangle_narrowed_rows={} textured_triangle_full_scan_rows={} constant_texel_textured_triangle_calls={} constant_texel_textured_triangle_white_texel_calls={} constant_texel_textured_triangle_non_white_texel_calls={} constant_texel_textured_triangle_white_alpha_only_eligible_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_rgb_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_uniform_rgb_calls={} constant_texel_textured_triangle_white_alpha_only_rejected_varying_rgb_calls={} constant_texel_textured_triangle_candidate_px={} constant_texel_textured_triangle_covered_px={} constant_texel_textured_triangle_white_texel_covered_px={} constant_texel_textured_triangle_white_endpoint_rows={} constant_texel_textured_triangle_white_endpoint_match_rows={} constant_texel_textured_triangle_white_endpoint_mismatch_rows={} constant_texel_textured_triangle_white_endpoint_empty_rows={} constant_texel_textured_triangle_white_endpoint_span_px={} constant_texel_textured_triangle_white_endpoint_probe_px={} constant_texel_textured_triangle_white_scan_runs={}", values[64], values[65], values[66], values[67], values[68], values[69], values[70], values[71], values[72], values[73], values[74], values[75], values[76], values[77], values[78], values[79], values[80], values[81], values[82], values[83]).expect("writing to a String cannot fail");
        write!(&mut line, " constant_texel_textured_triangle_white_scan_multi_run_rows={} constant_texel_textured_triangle_non_white_texel_covered_px={} constant_texel_textured_triangle_opaque_px={} constant_texel_textured_triangle_translucent_px={} constant_texel_textured_triangle_transparent_px={} constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls={} constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px={} constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls={} constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px={} constant_texel_textured_triangle_white_constant_alpha_run_calls={} constant_texel_textured_triangle_white_constant_alpha_run_px={} constant_texel_textured_triangle_white_constant_color_run_calls={} constant_texel_textured_triangle_white_constant_color_run_px={} constant_texel_textured_triangle_white_variable_color_run_calls={} constant_texel_textured_triangle_white_variable_color_run_px={} constant_texel_textured_triangle_white_variable_alpha_run_calls={} constant_texel_textured_triangle_white_variable_alpha_run_px={} constant_texel_textured_triangle_us={} constant_texel_textured_triangle_white_texel_us={} constant_texel_textured_triangle_non_white_texel_us={}", values[84], values[85], values[86], values[87], values[88], values[89], values[90], values[91], values[92], values[93], values[94], values[95], values[96], values[97], values[98], values[99], values[100], values[101], values[102], values[103]).expect("writing to a String cannot fail");
        write!(&mut line, " sampled_textured_triangle_calls={} sampled_textured_triangle_candidate_px={} sampled_textured_triangle_covered_px={} sampled_textured_triangle_us={} degenerate_triangle_skips={} fully_clipped_triangle_skips={} opaque_px={} translucent_px={} transparent_px={}", values[104], values[105], values[106], values[107], values[108], values[109], values[110], values[111], values[112]).expect("writing to a String cannot fail");
        line
    }
}

#[cfg(test)]
mod tests {
    use super::RasterStats;

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
                constant_texel_textured_triangle_white_scan_runs: 84,
                constant_texel_textured_triangle_white_scan_multi_run_rows: 85,
                constant_texel_textured_triangle_non_white_texel_covered_px: 86,
                constant_texel_textured_triangle_opaque_px: 87,
                constant_texel_textured_triangle_translucent_px: 88,
                constant_texel_textured_triangle_transparent_px: 89,
                constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_calls: 90,
                constant_texel_textured_triangle_white_alpha_only_constant_alpha_run_px: 91,
                constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_calls: 92,
                constant_texel_textured_triangle_white_alpha_only_variable_alpha_run_px: 93,
                constant_texel_textured_triangle_white_constant_alpha_run_calls: 94,
                constant_texel_textured_triangle_white_constant_alpha_run_px: 95,
                constant_texel_textured_triangle_white_constant_color_run_calls: 96,
                constant_texel_textured_triangle_white_constant_color_run_px: 97,
                constant_texel_textured_triangle_white_variable_color_run_calls: 98,
                constant_texel_textured_triangle_white_variable_color_run_px: 99,
                constant_texel_textured_triangle_white_variable_alpha_run_calls: 100,
                constant_texel_textured_triangle_white_variable_alpha_run_px: 101,
                constant_texel_textured_triangle_us: 102,
                constant_texel_textured_triangle_white_texel_us: 103,
                constant_texel_textured_triangle_non_white_texel_us: 104,
                sampled_textured_triangle_calls: 105,
                sampled_textured_triangle_candidate_px: 106,
                sampled_textured_triangle_covered_px: 107,
                sampled_textured_triangle_us: 108,
                degenerate_triangle_skips: 109,
                fully_clipped_triangle_skips: 110,
                opaque_px: 111,
                translucent_px: 112,
                transparent_px: 113,
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
            .map(|entry| {
                entry
                    .split_once('=')
                    .expect("stats entry has separator")
                    .1
                    .parse::<usize>()
                    .expect("stats value is numeric")
            })
            .collect();

        assert!(line.starts_with("software renderer raster_stats solid_rect_calls=1 "));
        assert!(line.contains(" textured_rect_separable_uv_calls=11 "));
        assert!(line.contains(" textured_rect_nonseparable_uv_px=14 "));
        assert_eq!(actual_values, (1..=113).collect::<Vec<_>>());
    }
}
