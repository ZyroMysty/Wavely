<script>
  import Icon from "./Icon.svelte";
</script>

<section>
  <Icon name="youtube" size={56} />
  <h1>YouTube Music</h1>
  <p>Kommt im nächsten Schritt: Anmeldung, deine Playlists, Likes und Suche – im Wavely-Player.</p>
</section>

<style>
  section {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 8px;
    color: var(--text-dim);
    padding: 32px;
  }
  h1 {
    margin: 8px 0 0;
    color: var(--text);
  }
  p {
    margin: 0;
    max-width: 380px;
  }
</style>
