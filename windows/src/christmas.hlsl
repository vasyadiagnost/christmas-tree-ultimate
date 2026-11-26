// A separate constant buffer avoids coupling Christmas decorations to snow or Halloween lighting.
cbuffer Christmas : register(b3) {
    float2 holidayView;
    float holidayTime;
    float holidayBrightness;
    float2 holidayAnchor;
    float2 holidaySize;
    float4 holidayTint;
    // World rotation, facing (+/-1), primitive kind, transport gait phase.
    float4 holidayMotion;
    // Blinking mode, light index / edge, beacon flag, reserved.
    float4 holidayExtra;
    float4 holidayUV;
};

Texture2D<float4> holidayPicture : register(t5);
SamplerState holidaySampler : register(s1);

struct ChristmasOut {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

ChristmasOut christmasVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = (corner - float2(0.5, 0.0)) * holidaySize;
    local.x *= holidayMotion.y;
    float sn = sin(holidayMotion.x), cs = cos(holidayMotion.x);
    local = float2(local.x * cs - local.y * sn, local.x * sn + local.y * cs);
    ChristmasOut result;
    result.position = float4((holidayAnchor + local) / holidayView * 2.0 - 1.0, 0.0, 1.0);
    result.uv = float2(corner.x, 1.0 - corner.y);
    return result;
}

float holidayBlink(float index) {
    float t = holidayTime;
    if (holidayExtra.z > 0.5)
        return 0.35 + 0.65 * pow(0.5 + 0.5 * sin(t * 9.0), 2.0);
    if (holidayExtra.x < 0.5)
        return 1.0;
    if (holidayExtra.x < 1.5)
        return 0.55 + 0.45 * sin(t * 1.6 + index * 1.71);
    float wave = 0.5 + 0.5 * sin(index * 0.7 - t * 3.2);
    return 0.25 + 0.75 * pow(wave, 4.0);
}

float2 holidayWheelUV(float2 uv, float2 center, float radius) {
    float2 delta = (uv - center) * float2(2.0, 1.0);
    float weight = 1.0 - smoothstep(radius - 0.014, radius, length(delta));
    float sn = sin(holidayMotion.w * 4.0), cs = cos(holidayMotion.w * 4.0);
    float2 rotated = float2(delta.x * cs - delta.y * sn, delta.x * sn + delta.y * cs);
    return lerp(uv, center + rotated / float2(2.0, 1.0), weight);
}

float4 christmasSprite(ChristmasOut i) : SV_Target {
    float2 uv = i.uv;
    if (holidayMotion.z > 0.5 && holidayMotion.z < 1.5) {
        // Only the interiors of the circular tires rotate; body and fenders stay fixed.
        uv = holidayWheelUV(uv, float2(0.161, 0.758), 0.237);
        uv = holidayWheelUV(uv, float2(0.507, 0.831), 0.164);
    }
    // Santa's legs are separate rigid painted parts driven by a continuous CPU rig.
    // No pixel strips or discontinuous UV warps are applied to any reindeer layer.
    float2 pictureUV = holidayUV.xy + uv * holidayUV.zw;
    float4 color = holidayPicture.Sample(holidaySampler, pictureUV) * holidayTint;
    if (holidayMotion.z > 1.5 && holidayMotion.z < 2.5) {
        // Rein-only transparent gaps in the original atlas. Replace their long
        // strokes with geometry, leaving hands, bodies and painted harness intact.
        float2 at = pictureUV * float2(1983.0, 793.0);
        float upper = 332.0 + (at.x - 660.0) * (70.0 / 222.0);
        float lower = 359.5 + (at.x - 660.0) * (82.0 / 222.0);
        bool frontGap = at.x > 660.0 && at.x < 882.0 && (abs(at.y - upper) < 13.0 || abs(at.y - lower) < 13.0);
        bool middleGap = at.x > 1320.0 && at.x < 1460.0 && at.y > 430.0 && at.y < 530.0;
        if (frontGap || middleGap) color = 0.0;
    }
    return color;
}

float4 christmasPrimitive(ChristmasOut i) : SV_Target {
    float kind = holidayMotion.z;
    float2 p = float2(i.uv.x, 1.0 - i.uv.y) * holidaySize;
    if (kind < 3.5) {
        // A gently sagging continuous green cord along three physical monitor edges.
        float edge = holidayExtra.y;
        float along = edge < 0.5 ? p.x : p.y;
        float sag = 0.5 - 0.5 * cos(along * 0.05609987);
        float distance;
        if (edge < 0.5)
            distance = abs(p.y - (26.0 - 8.0 * sag));
        else if (edge < 1.5)
            distance = abs(p.x - (8.0 + 5.0 * sag));
        else
            distance = abs(p.x - (20.0 - 5.0 * sag));
        float alpha = 1.0 - smoothstep(0.7, 1.5, distance);
        return float4(float3(0.035, 0.090, 0.055) * alpha, alpha * 0.94);
    }
    if (kind < 4.5) {
        float2 q = (i.uv - 0.5) * holidaySize;
        float radius = holidayExtra.w;
        float ellipse = length(q / float2(radius * 0.78, radius));
        float body = 1.0 - smoothstep(0.83, 1.08, ellipse);
        float glow = exp(-dot(q, q) / (radius * radius * 5.0));
        float brightness = holidayBrightness * holidayBlink(holidayExtra.y);
        float alpha = saturate(body * 0.90 + glow * 0.32 * brightness);
        float3 color = holidayTint.rgb * (0.48 * body + 0.58 * glow) * brightness;
        color += float3(1.0, 0.95, 0.78) * body * 0.35 * brightness;
        color = min(color, alpha.xxx);
        // The small green socket follows the same cable as its bulb.
        float socket = (1.0 - smoothstep(radius * 0.42, radius * 0.60, abs(q.x)))
            * smoothstep(-radius * 1.5, -radius * 1.15, q.y)
            * (1.0 - smoothstep(-radius * 0.9, -radius * 0.65, q.y));
        return float4(color * (1.0 - socket) + float3(0.04, 0.10, 0.055) * socket,
            saturate(alpha + socket * (1.0 - alpha)));
    }
    if (kind < 5.5) {
        // Placement mode: a restrained dashed border, always inside the hit-test rectangle.
        float2 edge = min(p, holidaySize - p);
        float border = 1.0 - smoothstep(1.0, 2.0, min(edge.x, edge.y));
        float stripe = step(0.35, frac((p.x + p.y) / 12.0));
        float alpha = border * stripe * 0.8;
        return float4(float3(1.0, 0.81, 0.34) * alpha, alpha);
    }
    if (kind > 6.5) {
        // Brown rein bridges join the original painted fragments across independently moving crops.
        float2 q = (i.uv - 0.5) * holidaySize;
        float radius = holidayExtra.w;
        float distance = length(float2(max(abs(q.x) - (holidaySize.x * 0.5 - radius), 0.0), q.y));
        float alpha = 1.0 - smoothstep(radius - 0.4, radius + 0.5, distance);
        float shade = 0.65 + 0.35 * saturate(1.0 - abs(q.y) / max(radius, 0.1));
        return float4(holidayTint.rgb * shade * alpha, alpha);
    }
    // Snow thrown forward by the plow is decorative dust, never re-deposited into the cleared drift.
    float2 q = float2(i.uv.x, 1.0 - i.uv.y);
    float snow = 0.0;
    [unroll] for (int k = 0; k < 18; ++k) {
        float seed = float(k);
        float age = frac(seed * 0.61803399 + holidayTime * 0.85);
        float2 at = float2(0.12 + age * 0.78, 0.10 + sin(age * 2.75) * (0.45 + frac(seed * 0.37) * 0.4));
        float2 d = (q - at) * holidaySize;
        float radius = 0.9 + frac(seed * 0.47) * 1.4;
        snow += exp(-dot(d, d) / (radius * radius)) * (1.0 - age) * 0.75;
    }
    float alpha = saturate(snow);
    return float4(float3(0.85, 0.94, 1.0) * alpha, alpha);
}
