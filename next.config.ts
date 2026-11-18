import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  output: 'export',
  devIndicators: false,
  transpilePackages: ['geist'],
  images: {
    unoptimized: true,
  },
}

export default nextConfig
