#!/usr/bin/env python3
"""
Generate weather forecast visualization video for Grandvoir (Belgium) and Vianden (Luxembourg).
Runs entirely in GitHub Actions - no local dependencies needed.
"""

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import matplotlib.animation as animation
import numpy as np
import os

# Forecast data (hardcoded from our search results)
GRANDVOIR = {
    "name": "Grandvoir, Belgium",
    "days": [
        ("Wed 26", 17, 25, "Partly cloudy", 21),
        ("Thu 27", 15, 27, "PM thunderstorms", 40),
        ("Fri 28", 12, 19, "Thunderstorms", 61),
        ("Sat 29", 14, 18, "Light rain", 52),
        ("Sun 30", 12, 21, "Showers", 56),
        ("Mon 31", 10, 18, "Showers", 64),
        ("Tue 1", 12, 21, "Partly cloudy", 21),
    ]
}

VIANDEN = {
    "name": "Vianden, Luxembourg",
    "days": [
        ("Wed 26", 17, 26, "Partly cloudy", 16),
        ("Thu 27", 17, 28, "Mostly cloudy", 17),
        ("Fri 28", 14, 21, "Rain & thunder", 56),
        ("Sat 29", 16, 19, "Showers", 42),
        ("Sun 30", 14, 22, "AM showers", 36),
        ("Mon 31", 12, 19, "Showers", 54),
        ("Tue 1", 13, 22, "Partly cloudy", 17),
    ]
}

COLORS = {
    'bg': '#0f172a',
    'card': '#1e293b',
    'accent': '#10b981',
    'accent_dim': '#10b98140',
    'text': '#f1f5f9',
    'text_dim': '#94a3b8',
    'high': '#f97316',
    'low': '#3b82f6',
    'rain': '#60a5fa',
    'sun': '#fbbf24',
    'storm': '#f43f5e',
}

def get_condition_color(condition):
    condition = condition.lower()
    if 'thunder' in condition or 'storm' in condition:
        return COLORS['storm']
    if 'rain' in condition or 'shower' in condition:
        return COLORS['rain']
    if 'cloud' in condition:
        return COLORS['text_dim']
    return COLORS['sun']

def create_frame(fig, axs, day_idx, locations, max_days):
    for ax, loc in zip(axs, locations):
        ax.clear()
        ax.set_facecolor(COLORS['bg'])
        fig.patch.set_facecolor(COLORS['bg'])

        days = loc['days']
        name = loc['name']

        # Show days up to current frame
        visible_days = days[:day_idx + 1]
        x_pos = np.arange(len(visible_days))
        labels = [d[0] for d in visible_days]
        highs = [d[2] for d in visible_days]
        lows = [d[1] for d in visible_days]
        conditions = [d[3] for d in visible_days]
        rain_chance = [d[4] for d in visible_days]

        # Temperature bars (high-low range)
        bar_width = 0.6
        for i, (low, high, cond, rain) in enumerate(zip(lows, highs, conditions, rain_chance)):
            color = get_condition_color(cond)
            # Background range bar
            ax.barh(i, high - low, left=low, height=bar_width,
                    color=COLORS['card'], edgecolor=color, linewidth=1.5,
                    alpha=0.3, zorder=1)
            # Filled portion up to current "temperature" (animated)
            fill_ratio = min(1.0, (day_idx + 1) / max_days * 1.5)
            fill_width = (high - low) * fill_ratio
            ax.barh(i, fill_width, left=low, height=bar_width,
                    color=color, alpha=0.7, zorder=2)

            # Low/High labels
            ax.text(low - 1.5, i, f'{low}°', ha='right', va='center',
                    fontsize=11, fontweight='bold', color=COLORS['low'], zorder=3)
            ax.text(high + 1.5, i, f'{high}°', ha='left', va='center',
                    fontsize=11, fontweight='bold', color=COLORS['high'], zorder=3)

            # Rain chance
            ax.text(high + 8, i, f'💧 {rain}%', ha='left', va='center',
                    fontsize=10, color=COLORS['rain'], zorder=3)

            # Condition icon/text
            icon = '🌤️' if 'cloud' in cond.lower() and 'thunder' not in cond.lower() else \
                   '⛈️' if 'thunder' in cond.lower() or 'storm' in cond.lower() else \
                   '🌧️' if 'rain' in cond.lower() or 'shower' in cond.lower() else '☀️'
            ax.text(low - 8, i, f'{icon} {cond}', ha='right', va='center',
                    fontsize=9, color=COLORS['text_dim'], zorder=3)

        # Day labels on y-axis
        ax.set_yticks(x_pos)
        ax.set_yticklabels(labels, fontsize=12, color=COLORS['text'], fontweight='medium')
        ax.invert_yaxis()

        # Title
        ax.set_title(name, fontsize=16, fontweight='bold', color=COLORS['text'], pad=20)

        # X-axis (temperature)
        ax.set_xlim(5, 40)
        ax.set_xticks(range(5, 41, 5))
        ax.set_xticklabels([f'{t}°C' for t in range(5, 41, 5)],
                           fontsize=9, color=COLORS['text_dim'])
        ax.tick_params(axis='x', length=0)
        ax.tick_params(axis='y', length=0)

        # Grid
        ax.set_axisbelow(True)
        ax.xaxis.grid(True, color=COLORS['card'], linewidth=0.5, alpha=0.5)
        ax.yaxis.grid(False)

        # Remove spines
        for spine in ax.spines.values():
            spine.set_visible(False)

        # Progress indicator
        progress = (day_idx + 1) / max_days
        ax.axvline(x=5 + 35 * progress, color=COLORS['accent'], linewidth=2, alpha=0.6, zorder=4)

    # Overall title
    fig.suptitle('Weekly Weather Forecast', fontsize=22, fontweight='bold',
                 color=COLORS['text'], y=0.95)
    fig.text(0.5, 0.91, f'Day {day_idx + 1} of {max_days}',
             ha='center', fontsize=12, color=COLORS['text_dim'])

    plt.tight_layout(rect=[0, 0.03, 1, 0.88])

def generate_video():
    locations = [GRANDVOIR, VIANDEN]
    max_days = max(len(loc['days']) for loc in locations)

    fig, axs = plt.subplots(1, 2, figsize=(19.2, 10.8), dpi=100)
    fig.patch.set_facecolor(COLORS['bg'])

    def animate(frame):
        create_frame(fig, axs, frame, locations, max_days)
        return axs

    # Create animation
    anim = animation.FuncAnimation(
        fig, animate, frames=max_days, interval=1500, repeat=False, blit=False
    )

    # Save as MP4
    output_path = 'weather_forecast.mp4'
    writer = animation.FFMpegWriter(fps=2, bitrate=5000, codec='libx264')
    anim.save(output_path, writer=writer, dpi=100)
    plt.close()

    file_size = os.path.getsize(output_path) / (1024 * 1024)
    print(f'Video saved: {output_path} ({file_size:.1f} MB)')
    return output_path

if __name__ == '__main__':
    generate_video()