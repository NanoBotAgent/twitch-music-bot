export default function Logo({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" fill="none" aria-hidden="true">
      <defs>
        <linearGradient id="tmb-logo-bg" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
          <stop stopColor="#34d399" />
          <stop offset="1" stopColor="#059669" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="9" fill="url(#tmb-logo-bg)" />
      <rect x="7" y="11" width="4" height="10" rx="2" fill="white" opacity="0.8" />
      <rect x="14" y="8" width="4" height="16" rx="2" fill="white" />
      <rect x="21" y="10" width="4" height="12" rx="2" fill="white" opacity="0.8" />
    </svg>
  );
}
