<!-- design-guard-allow-file colour: draws a made-up app, which has its own colours -->
<script lang="ts">
  /**
   * A fictional app "inside" a recording, drawn in a 1440×900 capture space and scaled to fit its box. Variant 0 is
   * the sample project's sign-up page and changes with time (typing, the success state) so playback has something
   * real to show. The real app shows decoded video frames here instead.
   */
  import { env } from '@shotmaxxing/ui';
  import '../lib/scenes.css';
  import { NORTHWIND } from '../lib/sample';

  let { variant = 0, t = 42, width }: { variant?: number; t?: number; width: number } = $props();

  const scale = $derived(width / 1440);
  // Northwind, the sample's sign-up page. Every state comes from `t`, so playing, seeking and exporting agree.
  const N = NORTHWIND;
  const clamp01 = (v: number) => Math.max(0, Math.min(1, v));
  const ramp = (a: number, b: number) => clamp01((t - a) / (b - a));
  const out = (u: number) => 1 - (1 - u) ** 3;
  const springy = (u: number) => (u <= 0 ? 0 : u >= 1 ? 1 : 1 + 2.7 * (u - 1) ** 3 + 1.7 * (u - 1) ** 2);
  const typedCount = $derived(
    Math.max(0, Math.min(N.address.length, Math.floor((t - N.typeStart) / N.typeStep) + 1)),
  );
  const typed = $derived(N.address.slice(0, t >= N.typeStart ? typedCount : 0));
  const typeEnd = N.typeStart + N.typeStep * N.address.length;
  const emailOn = $derived(t >= N.emailFocus && t < N.passwordFocus);
  const passwordOn = $derived(t >= N.passwordFocus && t < N.submit);
  const dots = $derived(Math.max(0, Math.min(10, Math.floor((t - N.passwordStart) / N.passwordStep) + 1)));
  // The caret stays solid while typing and blinks while waiting.
  const caretOn = $derived(t < typeEnd + 0.3 || Math.floor((t - typeEnd) * 1.9) % 2 === 0);
  const emailRing = $derived(emailOn ? out(ramp(N.emailFocus, N.emailFocus + 0.18)) : 0);
  const passwordRing = $derived(passwordOn ? out(ramp(N.passwordFocus, N.passwordFocus + 0.18)) : 0);
  // Get started: hovered as the cursor arrives, pressed on the click. Then the card takes the preview's place.
  const startHover = $derived(t >= N.reveal - 0.4 && t < N.reveal + 0.5);
  const startPress = $derived(t >= N.reveal && t < N.reveal + 0.14);
  const reveal = $derived(out(ramp(N.reveal + 0.1, N.reveal + 0.55)));
  // Create account: hover, press, a short wait, then signed up.
  const createHover = $derived(t >= N.submit - 0.35 && t < N.submit);
  const createPress = $derived(t >= N.submit && t < N.submit + 0.14);
  const creating = $derived(t >= N.submit + 0.12 && t < N.done);
  const done = $derived(t >= N.done);
  const success = $derived(springy(ramp(N.done, N.done + 0.55)));
  const form = $derived(1 - out(ramp(N.done - 0.05, N.done + 0.2)));
  // The cursor rests on Pricing near the end.
  const pricingHover = $derived(t >= N.pricingHover[0] && t < N.pricingHover[1]);
  const spin = $derived((t * 360 * 1.4) % 360);
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
    <div class="nw">
      <div class="nw-glow a"></div>
      <div class="nw-glow b"></div>
      <div class="nw-dots"></div>

      <div class="nw-bar"></div>
      {@render controls()}
      <div class="nw-url t"><span class="nw-lock"></span>northwind.app</div>

      <div class="nw-mark"></div>
      <div class="nw-name t">Northwind</div>
      {#each [['Product', 360], ['Pricing', 446], ['Customers', 548], ['Changelog', 660]] as [label, x] (label)}
        <div class="nw-link t" class:hover={label === 'Pricing' && pricingHover} style="left:{x}px">
          {label}
        </div>
      {/each}
      <div class="nw-link t" style="left:1172px">Sign in</div>
      <div class="nw-start t" class:hover={startHover} style="transform:scale({startPress ? 0.95 : 1})">
        Get started
      </div>

      <div class="nw-pill t">
        <span class="nw-spark">✦</span>New · AI meeting notes <span class="nw-arrow">→</span>
      </div>
      <div class="nw-h1">Notes that write <span class="nw-grad">themselves.</span></div>
      <div class="nw-lede">
        Northwind listens to your meetings, then writes the summary, the decisions and who does what next.
      </div>
      <div class="nw-faces">
        {#each faces as face, i (i)}<div
            class="nw-face"
            style="left:{i * 30}px;background:{face}"
          ></div>{/each}
        <div class="nw-stars t">★★★★★</div>
        <div class="nw-loved t">Loved by 12,000 teams</div>
      </div>
      <div class="nw-trust t">Trusted by teams at</div>
      <div class="nw-logo t" style="left:120px;font-weight:800;letter-spacing:0.12em">LUMEN</div>
      <div class="nw-logo t" style="left:250px;font-style:italic;font-weight:600">Atlas</div>
      <div class="nw-logo t" style="left:354px;font-weight:300;letter-spacing:0.02em">vela</div>
      <div class="nw-logo t" style="left:446px;font-weight:700;letter-spacing:0.2em;font-size:16px;top:812px">
        HALCYON
      </div>
      <div class="nw-logo t" style="left:598px;font-weight:600">
        orbit<span style="position:relative;color:oklch(0.62 0.17 270)">.</span>
      </div>

      <!-- Before Get started: a look at the product. -->
      <div
        class="nw-preview"
        style="opacity:{1 - reveal};transform:translateY({-12 * reveal}px) scale({1 - 0.03 * reveal})"
      >
        <div class="nw-pv-dot" style="left:24px"></div>
        <div class="nw-pv-dot" style="left:42px"></div>
        <div class="nw-pv-dot" style="left:60px"></div>
        <div class="t" style="left:32px;top:58px;font-size:24px;font-weight:700;letter-spacing:-0.02em">
          Weekly sync
        </div>
        <div class="t" style="left:32px;top:94px;font-size:14px;color:oklch(0.55 0.015 260)">
          Tue 3 Nov · 32 min · 5 people
        </div>
        <div class="nw-summary">
          <div class="t" style="left:20px;top:16px;font-size:13px;font-weight:700;color:oklch(0.5 0.17 275)">
            ✦ Summary
          </div>
          <div class="nw-pv-text" style="top:44px">
            The pricing page ships Friday, with the new team plan.
          </div>
          <div class="nw-pv-text" style="top:72px">Onboarding emails move to next sprint.</div>
          <div class="nw-pv-text" style="top:100px">Design reviews the Q4 roadmap on Thursday.</div>
        </div>
        <div class="t" style="left:32px;top:318px;font-size:13px;font-weight:700;color:oklch(0.45 0.015 260)">
          Action items
        </div>
        {#each ['Ship the pricing page', 'Draft the onboarding emails', 'Book the roadmap review'] as item, i (item)}
          <div class="nw-check" class:on={i === 0} style="top:{350 + i * 44}px"></div>
          <div
            class="t"
            style="left:66px;top:{350 + i * 44}px;font-size:16px;line-height:24px;color:oklch(0.3 0.02 260)"
          >
            {item}
          </div>
          <div class="nw-who t" style="top:{350 + i * 44}px">{['Ana', 'Maya', 'Leo'][i]}</div>
        {/each}
      </div>

      <!-- After: the sign-up card. -->
      <div class="nw-card" style="opacity:{reveal};transform:translateY({18 * (1 - reveal)}px)">
        <div class="nw-form" style="opacity:{form}">
          <div class="t" style="left:40px;top:38px;font-size:28px;font-weight:700;letter-spacing:-0.03em">
            Create your account
          </div>
          <div class="t" style="left:40px;top:80px;font-size:16px;color:oklch(0.52 0.015 260)">
            Free for teams up to 5. No card needed.
          </div>
          <div class="nw-sso t"><span class="nw-key"></span>Continue with SSO</div>
          <div class="nw-or"></div>
          <div class="nw-or-t t">or</div>
          <div class="nw-label t" style="top:226px">Name</div>
          <div class="nw-field t" style="top:248px">Maya Chen</div>
          <div class="nw-label t" style="top:310px">Work email</div>
          <div class="nw-field t" style="top:332px;--ring:{emailRing}">
            <span class="nw-typed">{typed}</span>{#if emailOn && caretOn}<span class="nw-caret"></span>{/if}
          </div>
          <div class="nw-label t" style="top:394px">Password</div>
          <div class="nw-field t" style="top:416px;--ring:{passwordRing}">
            <span class="nw-typed nw-pw">{'•'.repeat(t >= N.passwordStart ? dots : 0)}</span
            >{#if passwordOn && t < N.passwordStart + N.passwordStep * 10 + 0.3}<span class="nw-caret"
              ></span>{/if}
          </div>
          <div
            class="nw-create t"
            class:hover={createHover}
            style="transform:scale({createPress ? 0.97 : 1})"
          >
            {#if creating}<span class="nw-spin" style="transform:rotate({spin}deg)"></span>Creating your
              account…{:else}Create account{/if}
          </div>
          <div
            class="t"
            style="left:0;width:500px;top:556px;text-align:center;font-size:13px;color:oklch(0.58 0.01 260)"
          >
            By continuing you agree to the terms and privacy policy.
          </div>
        </div>
        {#if done}
          <div class="nw-done" style="opacity:{clamp01(success * 1.4)}">
            <div class="nw-ok" style="transform:scale({0.6 + 0.4 * success})">
              <svg viewBox="0 0 24 24" width="40" height="40" fill="none"
                ><path
                  d="M5 12.5l4.5 4.5L19 7.5"
                  stroke="currentColor"
                  stroke-width="2.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  pathLength="1"
                  stroke-dasharray="1"
                  stroke-dashoffset={1 - clamp01(ramp(N.done + 0.1, N.done + 0.5))}
                /></svg
              >
            </div>
            <div
              class="t"
              style="left:0;width:500px;top:262px;text-align:center;font-size:30px;font-weight:700;letter-spacing:-0.03em"
            >
              Check your inbox
            </div>
            <div
              class="t"
              style="left:0;width:500px;top:308px;text-align:center;font-size:17px;color:oklch(0.5 0.015 260)"
            >
              We sent a sign-in link to <b style="position:relative;color:oklch(0.28 0.02 260)">{N.address}</b
              >
            </div>
            <div class="nw-mail t">Open Mail</div>
            <div
              class="t"
              style="left:0;width:500px;top:446px;text-align:center;font-size:14px;color:oklch(0.6 0.01 260)"
            >
              Didn’t get it? Send it again in 30 s
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
