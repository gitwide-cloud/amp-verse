package cloud.gitwide.ampverse;

import android.app.*;
import android.widget.*;
import android.util.Xml;
import org.xmlpull.v1.XmlPullParser;
import org.json.*;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;

/** Public discovery and RSS browsing; playback/downloads belong to AMP Player. */
final class PodcastsPanel extends LinearLayout {
    final Activity a;final Appearance colors;
    final ExecutorService worker=Executors.newSingleThreadExecutor();
    final android.os.Handler handler=new android.os.Handler(android.os.Looper.getMainLooper());
    LinearLayout content;TextView status;volatile boolean closed;
    volatile JSONArray subscriptions = new JSONArray();
    volatile String job = "";
    private Runnable currentPage;
    private int pageVersion;
    PodcastsPanel(Activity a,Appearance colors){super(a);this.a=a;this.colors=colors;setOrientation(VERTICAL);setPadding(dp(8),dp(8),dp(8),dp(8));home();task(()->{JSONObject state=ModuleRpc.call(a,"module_read",obj("key","podcasts.subscriptions"));String saved=state.optString("value","[]");subscriptions=saved.equals("{}")?new JSONArray():new JSONArray(saved);return subscriptions.length()+" subscriptions · browse before subscribing";});}
    int dp(int n){return (int)(n*getResources().getDisplayMetrics().density);}
    static JSONObject obj(Object... values)throws JSONException{JSONObject j=new JSONObject();for(int i=0;i<values.length;i+=2)j.put(String.valueOf(values[i]),values[i+1]);return j;}
    TextView label(String text,int size){TextView v=new TextView(a);v.setText(text);v.setTextColor(colors.text);v.setTextSize(size);v.setPadding(dp(5),dp(6),dp(5),dp(6));return v;}
    Button button(String text,Runnable run){Button b=new Button(a);b.setText(text);b.setAllCaps(false);b.setTextColor(colors.text);b.setBackground(Chrome.button(colors,false));b.setMinHeight(dp(48));b.setOnClickListener(v->run.run());return b;}
    LinearLayout column(){LinearLayout v=new LinearLayout(a);v.setOrientation(VERTICAL);return v;}
    void row(LinearLayout parent,Button...buttons){LinearLayout row=new LinearLayout(a);for(Button b:buttons)row.addView(b,new LayoutParams(0,LayoutParams.WRAP_CONTENT,1));parent.addView(row);}
    void task(Callable<String> action){if(closed)return;worker.execute(()->{try{String result=action.call();a.runOnUiThread(()->{if(!closed)status.setText(result);});}catch(Exception error){a.runOnUiThread(()->{if(!closed)status.setText("Could not complete: "+error.getMessage()+". Try again.");});}});}
    void home(){removeAllViews();addView(label("ϟ AMP Podcasts",24));status=label("Browse public podcasts or paste an RSS feed. No account needed.",14);addView(status);
        row(this,button("Browse",()->search("popular")),button("Subscriptions",this::subscribed),button("Downloaded",this::downloads));
        LinearLayout search=column();EditText query=new EditText(a);query.setSingleLine(true);query.setHint("Podcast, episode topic or RSS URL");query.setTextColor(colors.text);query.setHintTextColor(colors.muted);query.setContentDescription("Search podcasts or enter an RSS feed");search.addView(query);row(search,button("Search",()->search(query.getText().toString())),button("Open RSS",()->show(query.getText().toString(),"RSS podcast")));addView(search);
        content=column();addView(content);categories();
    }
    void categories(){beginPage(this::categories);content.addView(label("Browse categories",20));String[] categories={"Arts","Business","Comedy","Education","Fiction","Health & Fitness","History","Music","News","Science","Society & Culture","Technology"};for(int i=0;i<categories.length;i+=2){final String left=categories[i],right=categories[i+1];row(content,button(left,()->search(left)),button(right,()->search(right)));}}
    static byte[] fetch(String address,int limit)throws Exception{
        String target=address;
        for(int redirects=0;redirects<5;redirects++){
            URI uri=new URI(target);if(!Arrays.asList("http","https").contains(uri.getScheme())||uri.getHost()==null||uri.getUserInfo()!=null)throw new IOException("Use a public HTTP or HTTPS feed");
            HttpURLConnection connection=(HttpURLConnection)uri.toURL().openConnection();connection.setInstanceFollowRedirects(false);connection.setConnectTimeout(8000);connection.setReadTimeout(15000);connection.setRequestProperty("User-Agent","AMPVerse/0.3.0 Podcast Reader");
            try{int code=connection.getResponseCode();if(code>=300&&code<400){String next=new URL(uri.toURL(),connection.getHeaderField("Location")).toString();if(target.startsWith("https://")&&!next.startsWith("https://"))throw new IOException("Insecure feed redirect");target=next;continue;}if(code!=200)throw new IOException("Feed server returned "+code);try(InputStream in=connection.getInputStream();ByteArrayOutputStream out=new ByteArrayOutputStream()){byte[] buffer=new byte[16384];int n;while((n=in.read(buffer))!=-1){if(Thread.currentThread().isInterrupted())throw new InterruptedIOException();if(out.size()+n>limit)throw new IOException("Feed is too large");out.write(buffer,0,n);}return out.toByteArray();}}finally{connection.disconnect();}
        }throw new IOException("Too many feed redirects");
    }
    private void beginPage(Runnable reopen) {
        pageVersion++;
        currentPage = reopen;
        content.removeAllViews();
        status.setText("");
    }

    private void loadPage(String loading, Callable<Runnable> load) {
        int request = ++pageVersion;
        status.setText(loading);
        worker.execute(() -> {
            try {
                Runnable render = load.call();
                a.runOnUiThread(() -> {
                    if (!closed && request == pageVersion) render.run();
                });
            } catch (Exception error) {
                a.runOnUiThread(() -> {
                    if (!closed && request == pageVersion) {
                        status.setText("Could not load podcasts: " + error.getMessage() + ". Try again.");
                    }
                });
            }
        });
    }

    void search(String query) {
        String term = query.trim();
        if (term.isEmpty()) {
            status.setText("Enter a topic or podcast name.");
            return;
        }
        loadPage("Searching public podcasts…", () -> {
            String url = "https://itunes.apple.com/search?media=podcast&entity=podcast&limit=30&term="
                    + URLEncoder.encode(term, "UTF-8");
            JSONObject response = new JSONObject(new String(fetch(url, 4 * 1024 * 1024), StandardCharsets.UTF_8));
            JSONArray results = response.getJSONArray("results");
            return () -> showResults(term, results);
        });
    }

    private void showResults(String term, JSONArray results) {
        beginPage(() -> showResults(term, results));
        content.addView(button("Back to categories", this::categories));
        content.addView(label("Results for " + term, 20));
        int count = 0;
        for (int i = 0; i < results.length(); i++) {
            JSONObject result = results.optJSONObject(i);
            if (result == null || result.optString("feedUrl").isEmpty()) continue;
            String title = result.optString("collectionName", "Podcast");
            String author = result.optString("artistName");
            String caption = author.isEmpty() ? title : title + "\n" + author;
            content.addView(button(caption, () -> show(result.optString("feedUrl"), title)));
            count++;
        }
        status.setText(count == 0 ? "No shows found. Try a different search." : count + " shows");
    }

    static JSONArray episodes(byte[] data)throws Exception{
        String text=new String(data,StandardCharsets.UTF_8);if(text.toUpperCase(Locale.ROOT).contains("<!DOCTYPE"))throw new IOException("RSS document declarations are not supported");
        XmlPullParser parser=Xml.newPullParser();parser.setInput(new StringReader(text));JSONArray episodes=new JSONArray();JSONObject item=null;String field="";StringBuilder value=new StringBuilder();
        for(int event=parser.getEventType();event!=XmlPullParser.END_DOCUMENT;event=parser.next()){
            String tag=parser.getName();if(event==XmlPullParser.START_TAG){if("item".equals(tag)||"entry".equals(tag)){item=new JSONObject();}if(item!=null){field=tag;value.setLength(0);if("enclosure".equals(tag)){String type=parser.getAttributeValue(null,"type"),url=parser.getAttributeValue(null,"url");if(url!=null&&(type==null||type.startsWith("audio/")))item.put("url",url);}else if("link".equals(tag)&&"enclosure".equals(parser.getAttributeValue(null,"rel")))item.put("url",parser.getAttributeValue(null,"href"));}}
            else if(event==XmlPullParser.TEXT&&item!=null){if(value.length()<20000)value.append(parser.getText());}
            else if(event==XmlPullParser.END_TAG&&item!=null){if(tag.equals(field)&&Arrays.asList("title","description","pubDate","itunes:duration").contains(tag))item.put(tag,value.toString().trim());if("item".equals(tag)||"entry".equals(tag)){String url=item.optString("url");if(url.startsWith("https://")||url.startsWith("http://"))episodes.put(item);item=null;if(episodes.length()>=200)break;}}
        }return episodes;
    }
    void show(String feed, String title) {
        String address = feed.trim();
        if (address.isEmpty()) {
            status.setText("Enter a podcast RSS URL.");
            return;
        }
        Runnable back = currentPage;
        loadPage("Loading episodes…", () -> {
            JSONArray rows = episodes(fetch(address, 4 * 1024 * 1024));
            return () -> showEpisodes(address, title, rows, back);
        });
    }

    private void showEpisodes(String feed, String title, JSONArray episodes, Runnable back) {
        beginPage(() -> showEpisodes(feed, title, episodes, back));
        content.addView(button("Back to podcasts", back));
        content.addView(label(title, 22));
        content.addView(button("Subscribe", () -> subscribe(feed, title)));
        status.setText(episodes.length() == 0 ? "This feed has no playable audio episodes." : episodes.length() + " episodes");
        for (int i = 0; i < episodes.length(); i++) {
            JSONObject episode = episodes.optJSONObject(i);
            String name = episode.optString("title", "Episode");
            String date = episode.optString("pubDate");
            content.addView(label(date.isEmpty() ? name : name + "\n" + date, 17));
            Button play = button("Play", () -> play(episode));
            Button download = button("Download", () -> download(episode));
            Button details = button("Details", () -> detail(episode));
            play.setContentDescription("Play " + name);
            download.setContentDescription("Download " + name);
            details.setContentDescription("Details for " + name);
            row(content, play, download, details);
        }
    }

    void detail(JSONObject episode){String description=android.text.Html.fromHtml(episode.optString("description"),android.text.Html.FROM_HTML_MODE_LEGACY).toString();new AlertDialog.Builder(a).setTitle(episode.optString("title")).setMessage(description.isEmpty()?"Public audio episode":description).setPositiveButton("Play",(d,n)->play(episode)).setNeutralButton("Download offline",(d,n)->download(episode)).setNegativeButton("Close",null).show();}
    void play(JSONObject episode){task(()->{String url=episode.getString("url");ModuleRpc.call(a,"player_stream",obj("url",url,"title",episode.optString("title"),"kind","amp-podcast:"+Integer.toHexString(url.hashCode())));return "Playing through AMP Player · "+episode.optString("title");});}
    void subscribe(String feed,String title){task(()->{JSONArray next=new JSONArray(subscriptions.toString());for(int i=0;i<next.length();i++)if(next.getJSONObject(i).optString("feed").equals(feed))return "Already subscribed";next.put(obj("feed",feed,"title",title));ModuleRpc.call(a,"module_write",obj("key","podcasts.subscriptions","value",next.toString()));subscriptions=next;return "Subscribed · "+title;});}
    void subscribed(){beginPage(this::subscribed);content.addView(label("Your subscriptions",20));if(subscriptions.length()==0)content.addView(label("No subscriptions yet. Browse and preview a show first.",15));for(int i=0;i<subscriptions.length();i++){JSONObject show=subscriptions.optJSONObject(i);content.addView(button(show.optString("title"),()->show(show.optString("feed"),show.optString("title"))));}}
    void download(JSONObject episode){task(()->{if(!job.isEmpty())return "A download is already in progress.";JSONObject response=ModuleRpc.call(a,"podcast_download",obj("url",episode.getString("url"),"title",episode.optString("title")));job=response.getString("job");handler.post(this::poll);return "Downloading episode in AMP Player…";});}
    void poll(){if(closed||job.isEmpty())return;task(()->{JSONObject result=ModuleRpc.call(a,"job",obj("job",job));if(result.optBoolean("done"))job="";else handler.postDelayed(this::poll,1000);return result.optString("status")+" "+result.optString("error");});}
    void downloads() {
        loadPage("Loading downloaded episodes…", () -> {
            JSONArray items = ModuleRpc.call(a, "podcast_downloads", null).getJSONArray("items");
            return () -> showDownloads(items);
        });
    }

    private void showDownloads(JSONArray items) {
        beginPage(() -> showDownloads(items));
        content.addView(label("Downloaded episodes", 20));
        if (!job.isEmpty()) {
            content.addView(button("Cancel current download", () -> task(() -> {
                String activeJob = job;
                if (activeJob.isEmpty()) return "No download is active.";
                ModuleRpc.call(a, "job", obj("job", activeJob, "cancel", true));
                return "Cancellation requested";
            })));
        }
        for (int i = 0; i < items.length(); i++) {
            JSONObject item = items.optJSONObject(i);
            if (item == null) continue;
            content.addView(button("Play " + item.optString("title"), () -> task(() -> {
                ModuleRpc.call(a, "podcast_offline", obj("id", item.getString("id")));
                return "Playing offline in AMP Player";
            })));
        }
        status.setText(items.length() == 0 ? "No episodes downloaded. Choose Download on an episode."
                : items.length() + " episodes available offline");
    }

    void close(){closed=true;handler.removeCallbacksAndMessages(null);worker.shutdownNow();}
}
