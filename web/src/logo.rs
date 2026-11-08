//! The Wayback Chain wordmark: original 1980s-style artwork (not any film
//! logo). Full-width slanted WAYBACK in a sunset gradient over an outlined
//! neon CHAIN badge, with speed streaks either side. Every element is
//! centred on x = 280 of the 560-wide viewBox (the slant is compensated).
//!
//! Static SVG markup (no user data). Text is sized with textLength so it
//! renders the same whatever heavy font the system has. The page heading
//! keeps the plain text "Wayback Chain" for screen readers and search
//! engines; this SVG is aria-hidden.

pub const SVG: &str = r##"<svg viewBox="0 0 560 190" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">
<defs>
  <linearGradient id="wcSun" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#fff59a"/><stop offset="0.42" stop-color="#ffb03a"/>
    <stop offset="0.5" stop-color="#ffe9c2"/><stop offset="0.54" stop-color="#ff8a3d"/>
    <stop offset="1" stop-color="#ff2fb3"/>
  </linearGradient>
  <linearGradient id="wcStreak" x1="0" y1="0" x2="1" y2="0">
    <stop offset="0" stop-color="#19e3ff" stop-opacity="0"/><stop offset="1" stop-color="#19e3ff"/>
  </linearGradient>
  <linearGradient id="wcStreakR" x1="1" y1="0" x2="0" y2="0">
    <stop offset="0" stop-color="#19e3ff" stop-opacity="0"/><stop offset="1" stop-color="#19e3ff"/>
  </linearGradient>
  <filter id="wcGlow" x="-20%" y="-50%" width="140%" height="200%">
    <feGaussianBlur stdDeviation="5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
  </filter>
</defs>
<g id="wcWord" transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="47" y="100" font-size="94" textLength="490" lengthAdjust="spacingAndGlyphs" fill="none" stroke="#19e3ff" stroke-width="11" stroke-linejoin="round" filter="url(#wcGlow)" opacity="0.55">WAYBACK</text>
  <text x="47" y="100" font-size="94" textLength="490" lengthAdjust="spacingAndGlyphs" fill="url(#wcSun)" stroke="#1b0a33" stroke-width="6.5" stroke-linejoin="round" paint-order="stroke">WAYBACK</text>
</g>
<path id="wcBadge" d="M173 122 L403 122 L387 178 L157 178 Z" fill="rgba(10,4,22,0.85)" stroke="#19e3ff" stroke-width="4" filter="url(#wcGlow)"/>
<g id="wcChain" transform="skewX(-10)" font-family="'Arial Black','Arial Bold',Impact,'Helvetica Neue',sans-serif" font-weight="900">
  <text x="216.5" y="166" font-size="40" textLength="180" lengthAdjust="spacing" fill="none" stroke="#19e3ff" stroke-width="6" opacity="0.6" filter="url(#wcGlow)">CHAIN</text>
  <text x="216.5" y="166" font-size="40" textLength="180" lengthAdjust="spacing" fill="#d9fbff">CHAIN</text>
</g>
<g id="wcStreaks">
  <g fill="url(#wcStreak)">
    <rect x="32" y="134" width="118" height="6" rx="3"/><rect x="66" y="148" width="84" height="6" rx="3"/><rect x="96" y="162" width="54" height="6" rx="3"/>
  </g>
  <g fill="url(#wcStreakR)">
    <rect x="410" y="134" width="118" height="6" rx="3"/><rect x="410" y="148" width="84" height="6" rx="3"/><rect x="410" y="162" width="54" height="6" rx="3"/>
  </g>
</g>
</svg>"##;
