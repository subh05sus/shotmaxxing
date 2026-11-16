import type { NextConfig } from 'next';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..', '..');

const config: NextConfig = {
  // The site reads the guides, the screenshots and install.sh from the rest of the repository.
  turbopack: { root },
  outputFileTracingRoot: root,
  poweredByHeader: false,
  reactStrictMode: true,
  // Next writes an AGENTS.md into the app otherwise; the repository has its own instructions.
  agentRules: false,
  images: { formats: ['image/avif', 'image/webp'] },
};

export default config;
