# Oracle calibration evidence

Status: candidate; maintainer approval required.

| case | pods | visibility | class | tenants | samples | p50 ms | p95 ms | p99 ms | p99 TTFB ms | rows/s | peak memory B | spill B | CPU s | source ms | tail ms | peak slots | retry rate | rejection rate | audits |
|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| p1-published_only-interactive-t1 | 1 | published_only | interactive | 1 | 20 | 40.845 | 43.040 | 43.389 | 43.008 | 49134.858 | 6585592 | 0 | 1.040 | 61.605 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p1-published_only-interactive-t2 | 1 | published_only | interactive | 2 | 40 | 42.868 | 46.142 | 49.924 | 49.416 | 46706.001 | 6585592 | 0 | 2.150 | 123.146 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p1-published_only-analytical-t1 | 1 | published_only | analytical | 1 | 20 | 30.760 | 31.928 | 33.661 | 33.209 | 64774.601 | 6585592 | 0 | 0.640 | 63.955 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p1-published_only-analytical-t2 | 1 | published_only | analytical | 2 | 40 | 31.553 | 34.073 | 57.756 | 57.211 | 61852.864 | 6585592 | 0 | 1.300 | 129.848 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p1-fused-interactive-t1 | 1 | fused | interactive | 1 | 20 | 48.096 | 51.574 | 57.075 | 56.383 | 41133.526 | 6647905 | 0 | 1.220 | 69.808 | 39.233 | 2 | 0.000000 | 1.000000 | true |
| p1-fused-interactive-t2 | 1 | fused | interactive | 2 | 40 | 50.861 | 54.021 | 56.406 | 55.669 | 39580.557 | 6647905 | 0 | 2.470 | 136.838 | 84.099 | 2 | 0.000000 | 1.000000 | true |
| p1-fused-analytical-t1 | 1 | fused | analytical | 1 | 20 | 37.748 | 45.319 | 56.052 | 55.355 | 51111.437 | 6647905 | 0 | 0.800 | 73.367 | 37.898 | 2 | 0.000000 | 1.000000 | true |
| p1-fused-analytical-t2 | 1 | fused | analytical | 2 | 40 | 42.223 | 46.747 | 67.707 | 66.878 | 46764.711 | 6647905 | 0 | 1.660 | 142.936 | 93.110 | 2 | 0.000000 | 1.000000 | true |
| p3-published_only-interactive-t1 | 3 | published_only | interactive | 1 | 20 | 51.882 | 55.032 | 56.811 | 55.898 | 39201.005 | 6585592 | 0 | 1.250 | 64.381 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p3-published_only-interactive-t2 | 3 | published_only | interactive | 2 | 40 | 50.663 | 57.084 | 72.106 | 71.251 | 39004.317 | 6585592 | 0 | 2.490 | 128.809 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p3-published_only-analytical-t1 | 3 | published_only | analytical | 1 | 20 | 34.441 | 40.286 | 42.063 | 41.265 | 55598.335 | 6585592 | 0 | 0.750 | 66.571 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p3-published_only-analytical-t2 | 3 | published_only | analytical | 2 | 40 | 37.671 | 44.247 | 46.035 | 45.097 | 53067.888 | 6585592 | 0 | 1.490 | 130.875 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p3-fused-interactive-t1 | 3 | fused | interactive | 1 | 20 | 47.906 | 58.562 | 76.863 | 75.875 | 39577.054 | 6647905 | 0 | 1.340 | 72.052 | 39.959 | 2 | 0.000000 | 1.000000 | true |
| p3-fused-interactive-t2 | 3 | fused | interactive | 2 | 40 | 51.089 | 56.354 | 58.835 | 57.826 | 39006.076 | 6647905 | 0 | 2.730 | 143.598 | 83.142 | 2 | 0.000000 | 1.000000 | true |
| p3-fused-analytical-t1 | 3 | fused | analytical | 1 | 20 | 37.894 | 39.399 | 40.374 | 39.145 | 52673.281 | 6647905 | 0 | 0.870 | 75.010 | 39.313 | 2 | 0.000000 | 1.000000 | true |
| p3-fused-analytical-t2 | 3 | fused | analytical | 2 | 40 | 37.928 | 39.907 | 72.396 | 71.206 | 51914.661 | 6647905 | 0 | 1.760 | 150.447 | 85.808 | 2 | 0.000000 | 1.000000 | true |
| p6-published_only-interactive-t1 | 6 | published_only | interactive | 1 | 20 | 56.866 | 61.063 | 65.608 | 64.429 | 35489.655 | 6585592 | 0 | 1.330 | 63.641 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p6-published_only-interactive-t2 | 6 | published_only | interactive | 2 | 40 | 60.338 | 66.714 | 76.400 | 75.287 | 33086.385 | 6585592 | 0 | 2.790 | 129.644 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p6-published_only-analytical-t1 | 6 | published_only | analytical | 1 | 20 | 35.496 | 40.921 | 57.081 | 55.535 | 54310.488 | 6585592 | 0 | 0.790 | 70.175 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p6-published_only-analytical-t2 | 6 | published_only | analytical | 2 | 40 | 41.033 | 48.298 | 66.554 | 64.938 | 48761.845 | 6585592 | 0 | 1.640 | 135.367 | -0.000 | 2 | 0.000000 | 1.000000 | true |
| p6-fused-interactive-t1 | 6 | fused | interactive | 1 | 20 | 48.683 | 51.077 | 51.344 | 49.940 | 40637.404 | 6647905 | 0 | 1.470 | 75.197 | 47.100 | 2 | 0.000000 | 1.000000 | true |
| p6-fused-interactive-t2 | 6 | fused | interactive | 2 | 40 | 50.427 | 59.187 | 79.222 | 77.837 | 38194.531 | 6647905 | 0 | 2.940 | 152.910 | 101.986 | 2 | 0.000000 | 1.000000 | true |
| p6-fused-analytical-t1 | 6 | fused | analytical | 1 | 20 | 39.457 | 41.229 | 41.945 | 40.701 | 50606.532 | 6647905 | 0 | 0.960 | 77.522 | 46.753 | 2 | 0.000000 | 1.000000 | true |
| p6-fused-analytical-t2 | 6 | fused | analytical | 2 | 40 | 39.142 | 41.505 | 46.116 | 44.679 | 50711.732 | 6647905 | 0 | 1.910 | 152.676 | 91.407 | 2 | 0.000000 | 1.000000 | true |

Proposal values are derived from the slowest measured case: observed resource-per-slot ratios, scan bytes/row and throughput, latency deltas, topology worker count, and recorder values. Zero spill, retry, or tail time means the production recorder observed no such activity in that case; it is not a fabricated floor or an approval claim. The candidate remains blocked on maintainer review of safety margins and workload representativeness.
