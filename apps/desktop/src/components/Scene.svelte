<!-- design-guard-allow-file colour: draws a made-up app, which has its own colours -->
<script lang="ts">
  /**
   * A fictional app "inside" a recording, drawn in a 1440×900 capture space and scaled to fit its box. Variant 0 is
   * the sample project's sign-up page and changes with time (typing, the success state) so playback has something
   * real to show. The real app shows decoded video frames here instead.
   */
  import { env } from '@shotmaxxing/ui';
  import '../lib/scenes.css';

  let { variant = 0, t = 42, width }: { variant?: number; t?: number; width: number } = $props();

  const scale = $derived(width / 1440);
  const email = 'maya@northwind.app';
  const typed = $derived(email.slice(0, Math.max(0, Math.min(email.length, Math.floor((t - 9.6) / 0.22)))));
  const typing = $derived(t >= 9.4 && t < 15.4);
  const dots = $derived(Math.max(0, Math.min(10, Math.floor((t - 15.6) / 0.18))));
  const done = $derived(t >= 20.2);
  const faces = ['oklch(0.8 0.1 30)', 'oklch(0.78 0.1 200)', 'oklch(0.82 0.12 90)', 'oklch(0.74 0.12 300)'];
</script>

<!-- The made-up browser's window buttons, as the system draws them: traffic lights on the Mac, caption buttons on
     Windows. -->
{#snippet controls()}
  {#if env.os === 'windows'}
    <div class="scn-cap min" style="left:1266px"></div>
    <div class="scn-cap max" style="left:1324px"></div>
    <div class="scn-cap close" style="left:1382px"></div>
  {:else}
    <div class="scn-dot r"></div>
    <div class="scn-dot y"></div>
    <div class="scn-dot g"></div>
  {/if}
{/snippet}

<div class="scn" class:scn-dark={variant === 1} style:transform="scale({scale})" aria-hidden="true">
  {#if variant === 1}
    <div class="scn-panel" style="left:0;top:0;width:1440px;height:56px"></div>
    <div class="scn-panel" style="left:0;top:56px;width:250px;height:844px"></div>
    <div class="scn-panel" style="left:1180px;top:56px;width:260px;height:844px"></div>
    {#each Array(9) as _, i (i)}
      <div class="scn-row" style="left:24px;top:{90 + i * 34}px;width:{140 + ((i * 37) % 60)}px"></div>
      <div class="scn-row" style="left:1204px;top:{90 + i * 40}px;width:{110 + ((i * 53) % 90)}px"></div>
    {/each}
    <div class="scn-frame" style="left:380px;top:150px;width:300px;height:620px"></div>
    <div class="scn-frame scn-sel" style="left:740px;top:150px;width:300px;height:620px"></div>
    <div class="scn-btn" style="left:420px;top:640px;width:220px;height:52px;line-height:52px;font-size:18px">
      Continue
    </div>
    <div
      class="scn-row"
      style="left:780px;top:220px;width:200px;height:26px;background:oklch(0.85 0.01 260)"
    ></div>
    <div class="scn-row" style="left:780px;top:270px;width:150px;background:oklch(0.9 0.005 260)"></div>
  {:else if variant === 2}
    <div class="scn-bar"></div>
    {@render controls()}
    <div
      class="scn-row"
      style="left:80px;top:140px;width:500px;height:22px;background:oklch(0.9 0.005 260)"
    ></div>
    <div
      class="scn-row"
      style="left:80px;top:190px;width:360px;height:14px;background:oklch(0.93 0.004 260)"
    ></div>
    <div class="scn-dialog">
      <div class="t" style="left:40px;top:44px;font-size:24px;font-weight:700">Exporting “Q3 review.mov”</div>
      <div class="t" style="left:40px;top:90px;font-size:16px;color:oklch(0.5 0.01 260)">
        About 1 second left…
      </div>
      <div class="scn-track"><div class="scn-fill" style="width:99%"></div></div>
      <div class="t" style="left:420px;top:176px;font-size:15px;font-weight:600">99%</div>
      <div
        class="scn-btn ghost"
        style="left:350px;top:200px;width:110px;height:40px;line-height:40px;font-size:16px"
      >
        Cancel
      </div>
    </div>
  {:else if variant === 3}
    <div class="scn-bar" style="height:110px"></div>
    <div class="t" style="left:30px;top:24px;font-size:20px;font-weight:700;color:oklch(0.4 0.12 150)">
      Budget 2026
    </div>
    <div
      class="scn-field"
      style="left:30px;top:62px;width:1380px;height:36px;line-height:33px;font-size:16px"
    >
      <span style="position:relative;color:oklch(0.5 0.01 260)">fx</span>&nbsp;&nbsp;=SUM(B2:B9)
    </div>
    <div class="scn-grid" style="left:0;top:110px;width:1440px;height:790px"></div>
    {#each ['', 'A', 'B', 'C', 'D', 'E', 'F', 'G'] as col, i (i)}
      <div class="scn-cell scn-head" style="left:{i * 180}px;top:110px">{col}</div>
    {/each}
    {#each ['Rent', 'Payroll', 'Tools', 'Ads', 'Travel', 'Office', 'Legal', 'Other'] as item, r (item)}
      <div class="scn-cell" style="left:180px;top:{146 + r * 36}px;text-align:left;padding-left:12px">
        {item}
      </div>
      <div class="scn-cell" style="left:360px;top:{146 + r * 36}px">
        {[2400, 18200, 940, 3100, 1250, 600, 1800, 410][r].toLocaleString('en-US')}
      </div>
    {/each}
    <div class="scn-cell" style="left:180px;top:434px;text-align:left;padding-left:12px;font-weight:700">
      Total
    </div>
    <div class="scn-cell scn-active" style="left:360px;top:434px;font-weight:700">28,700</div>
  {:else if variant === 4}
    <div class="scn-bar"></div>
    {@render controls()}
    <div class="scn-url t">northwind.app/pricing</div>
    <div
      class="t"
      style="left:0;width:1440px;top:150px;text-align:center;font-size:52px;font-weight:750;letter-spacing:-0.03em"
    >
      Simple pricing
    </div>
    {#each [['Free', '$0'], ['Team', '$12'], ['Business', '$29']] as [name, price], i (name)}
      <div class="scn-tier" class:hot={i === 1} style="left:{120 + i * 410}px">
        <div class="t" style="left:36px;top:36px;font-size:22px;font-weight:700">{name}</div>
        <div class="t" style="left:36px;top:80px;font-size:54px;font-weight:750">{price}</div>
        {#each [0, 1, 2, 3, 4] as b (b)}<div
            class="scn-bullet"
            style="top:{190 + b * 36}px;width:{200 + ((b * 41) % 80)}px"
          ></div>{/each}
        <div
          class="scn-btn"
          class:ghost={i !== 1}
          style="left:36px;top:380px;width:288px;height:48px;line-height:48px;font-size:17px"
        >
          Choose {name}
        </div>
      </div>
    {/each}
  {:else if variant === 5}
    <div class="scn-bar" style="background:oklch(0.97 0.01 85)"></div>
    {@render controls()}
    <div style="left:0;top:72px;width:1440px;height:828px;background:oklch(0.99 0.008 85)"></div>
    <div class="t" style="left:260px;top:140px;font-size:46px;font-weight:750;letter-spacing:-0.02em">
      Release notes 0.3
    </div>
    <div class="t" style="left:260px;top:210px;font-size:18px;color:oklch(0.55 0.01 85)">
      Tuesday, 3 November
    </div>
    {#each [0, 1, 2, 3, 4, 5, 6] as l (l)}
      <div
        style="left:260px;top:{290 +
          l * 58}px;width:12px;height:12px;border-radius:50%;background:oklch(0.7 0.12 85)"
      ></div>
      <div
        class="scn-row"
        style="left:292px;top:{289 + l * 58}px;width:{520 +
          ((l * 97) % 340)}px;height:14px;background:oklch(0.9 0.01 85)"
      ></div>
    {/each}
  {:else}
    <div class="scn-bar"></div>
    {@render controls()}
    <div class="scn-url t">northwind.app/signup</div>
    <div class="scn-logo t">Northwind</div>
    <div class="scn-link t" style="left:340px">Product</div>
    <div class="scn-link t" style="left:430px">Pricing</div>
    <div class="scn-link t" style="left:520px">Docs</div>
    <div class="scn-link t" style="left:1180px">Sign in</div>
    <div class="scn-btn" style="left:1250px;top:90px;width:110px;height:40px;line-height:40px;font-size:16px">
      Get started
    </div>
    <div class="scn-pill t">New · AI summaries</div>
    <div class="scn-h1">Notes that write themselves.</div>
    <div class="scn-p">
      Northwind turns meetings, links and late-night ideas into tidy notes your whole team can find.
    </div>
    <div class="scn-faces">
      {#each faces as face, i (i)}<div
          class="scn-face"
          style="left:{i * 28}px;background:{face}"
        ></div>{/each}
      <div class="t" style="left:132px;top:9px;font-size:16px;color:oklch(0.45 0.015 260)">
        Loved by 12,000 teams
      </div>
    </div>
    <div class="scn-card">
      {#if done}
        <div class="scn-check">✓</div>
        <div class="t" style="left:0;width:520px;top:230px;text-align:center;font-size:28px;font-weight:700">
          Check your inbox
        </div>
        <div
          class="t"
          style="left:0;width:520px;top:276px;text-align:center;font-size:17px;color:oklch(0.5 0.015 260)"
        >
          We sent a sign-in link to {email}
        </div>
      {:else}
        <div class="t" style="left:40px;top:36px;font-size:28px;font-weight:700;letter-spacing:-0.02em">
          Create your account
        </div>
        <div class="t" style="left:40px;top:76px;font-size:16px;color:oklch(0.5 0.015 260)">
          Free for teams up to 5. No card needed.
        </div>
        <div class="scn-label t" style="left:40px;top:114px">Name</div>
        <div class="scn-field t" style="top:136px">Maya Chen</div>
        <div class="scn-label t" style="left:40px;top:196px">Work email</div>
        <div class="scn-field t" class:focus={typing} style="top:218px">
          {typed}{#if typing}<span class="scn-caret"></span>{/if}
        </div>
        <div class="scn-label t" style="left:40px;top:278px">Password</div>
        <div class="scn-field t" class:focus={t >= 15.4 && t < 18.6} style="top:300px;letter-spacing:4px">
          {'•'.repeat(dots)}
        </div>
        <div
          class="scn-btn"
          style="left:40px;top:376px;width:440px;height:48px;line-height:48px;font-size:17px"
        >
          Create account
        </div>
        <div
          class="t"
          style="left:0;width:520px;top:446px;text-align:center;font-size:14px;color:oklch(0.55 0.01 260)"
        >
          By continuing you agree to the terms.
        </div>
      {/if}
    </div>
  {/if}
</div>
