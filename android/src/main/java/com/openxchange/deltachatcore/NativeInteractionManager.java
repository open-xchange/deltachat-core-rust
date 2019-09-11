package com.openxchange.deltachatcore;

import android.app.Activity;
import android.content.Context;
import android.os.PowerManager;
import android.util.Log;

import com.b44t.messenger.DcContext;
import com.b44t.messenger.DcEventCenter;

import java.io.BufferedInputStream;
import java.io.BufferedReader;
import java.io.File;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Map;
import java.util.Objects;

import static com.openxchange.deltachatcore.DeltaChatCorePlugin.TAG;

public class NativeInteractionManager extends DcContext {

    private static final String COI_IMAP_WAKE_LOCK = "coi:imapWakeLock";
    private static final String COI_MVBOX_WAKE_LOCK = "coi:mvboxWakeLock";
    private static final String COI_SENT_BOX_WAKE_LOCK = "coi:sentBoxWakeLock";
    private static final String COI_SMTP_WAKE_LOCK = "coi:smtpWakeLock";
    private static final int EVENT_ERROR = 400;
    private static final int INTERRUPT_IDLE = 0x01; // interrupt idle if the thread is already running

    private final Object threadsCritical = new Object();
    private final Object imapThreadStartedCond = new Object();
    private final Object mvboxThreadStartedCond = new Object();
    private final Object sentBoxThreadStartedCond = new Object();
    private final Object smtpThreadStartedCond = new Object();
    public DcEventCenter eventCenter = new DcEventCenter();
    private Thread imapThread = null;
    private PowerManager.WakeLock imapWakeLock = null;
    private Thread mvboxThread = null;
    private PowerManager.WakeLock mvboxWakeLock = null;
    private Thread sentBoxThread = null;
    private PowerManager.WakeLock sentBoxWakeLock = null;
    private Thread smtpThread = null;
    private PowerManager.WakeLock smtpWakeLock = null;
    private long wakeLockTimeout = 10 * 60 * 1000L; /*10 minutes*/
    private Map<Long, String> coreStrings;
    private String dbPath;


    NativeInteractionManager(Context context, Activity activity, String dbName) {
        super("Android " + BuildConfig.VERSION_NAME);

        File databaseFile = new File(context.getFilesDir(), dbName);
        dbPath = databaseFile.getAbsolutePath();
        open(databaseFile.getAbsolutePath());

        if (activity != null) {
            try {
                PowerManager powerManager = (PowerManager) activity.getSystemService(Context.POWER_SERVICE);

                imapWakeLock = Objects.requireNonNull(powerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, COI_IMAP_WAKE_LOCK);
                imapWakeLock.setReferenceCounted(false); // if the idle-thread is killed for any reasons, it is better not to rely on reference counting

                mvboxWakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, COI_MVBOX_WAKE_LOCK);
                mvboxWakeLock.setReferenceCounted(false); // if the idle-thread is killed for any reasons, it is better not to rely on reference counting

                sentBoxWakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, COI_SENT_BOX_WAKE_LOCK);
                sentBoxWakeLock.setReferenceCounted(false); // if the idle-thread is killed for any reasons, it is better not to rely on reference counting

                smtpWakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, COI_SMTP_WAKE_LOCK);
                smtpWakeLock.setReferenceCounted(false); // if the idle-thread is killed for any reasons, it is better not to rely on reference counting
            } catch (Exception e) {
                Log.e(TAG, "Cannot create wakeLocks");
            }

            start();
        }
    }

    public void start() {
        Log.d(TAG, "Starting threads");
        startThreads(0);
    }

    public void stop() {
        Log.d(TAG, "Stopping threads");
        stopThreads();
    }

    void setCoreStrings(Map<Long, String> coreStrings) {
        this.coreStrings = coreStrings;
    }

    private void startThreads(@SuppressWarnings("SameParameterValue") int flags) {
        synchronized (threadsCritical) {

            if (imapThread == null || !imapThread.isAlive()) {

                synchronized (imapThreadStartedCond) {
                }

                imapThread = new Thread(() -> {
                    // raise the starting condition
                    // after acquiring a wakelock so that the process is not terminated.
                    // as imapWakeLock is not reference counted that would result in a wakelock-gap is not needed here.
                    imapWakeLock.acquire(wakeLockTimeout);
                    synchronized (imapThreadStartedCond) {
                        imapThreadStartedCond.notifyAll();
                    }

                    Log.i(TAG, "###################### IMAP-Thread started. ######################");


                    while (true) {
                        if (Thread.interrupted()) {
                            return;
                        }
                        imapWakeLock.acquire(wakeLockTimeout);
                        performImapJobs();
                        performImapFetch();
                        imapWakeLock.release();
                        performImapIdle();
                    }
                }, "imapThread");
                imapThread.setPriority(Thread.NORM_PRIORITY);
                imapThread.start();
            } else {
                if ((flags & INTERRUPT_IDLE) != 0) {
                    interruptImapIdle();
                }
            }


            if (mvboxThread == null || !mvboxThread.isAlive()) {

                synchronized (mvboxThreadStartedCond) {
                }

                mvboxThread = new Thread(() -> {
                    mvboxWakeLock.acquire(wakeLockTimeout);
                    synchronized (mvboxThreadStartedCond) {
                        mvboxThreadStartedCond.notifyAll();
                    }

                    Log.i(TAG, "###################### MVBOX-Thread started. ######################");


                    while (true) {
                        if (Thread.interrupted()) {
                            return;
                        }
                        mvboxWakeLock.acquire(wakeLockTimeout);
                        performMvboxFetch();
                        mvboxWakeLock.release();
                        performMvboxIdle();
                    }
                }, "mvboxThread");
                mvboxThread.setPriority(Thread.NORM_PRIORITY);
                mvboxThread.start();
            } else {
                if ((flags & INTERRUPT_IDLE) != 0) {
                    interruptMvboxIdle();
                }
            }


            if (sentBoxThread == null || !sentBoxThread.isAlive()) {

                synchronized (sentBoxThreadStartedCond) {
                }

                sentBoxThread = new Thread(() -> {
                    sentBoxWakeLock.acquire(wakeLockTimeout);
                    synchronized (sentBoxThreadStartedCond) {
                        sentBoxThreadStartedCond.notifyAll();
                    }

                    Log.i(TAG, "###################### SENTBOX-Thread started. ######################");


                    while (true) {
                        if (Thread.interrupted()) {
                            return;
                        }
                        sentBoxWakeLock.acquire(wakeLockTimeout);
                        performSentboxFetch();
                        sentBoxWakeLock.release();
                        performSentboxIdle();
                    }
                }, "sentBoxThread");
                sentBoxThread.setPriority(Thread.NORM_PRIORITY - 1);
                sentBoxThread.start();
            } else {
                if ((flags & INTERRUPT_IDLE) != 0) {
                    interruptSentboxIdle();
                }
            }


            if (smtpThread == null || !smtpThread.isAlive()) {

                synchronized (smtpThreadStartedCond) {
                }

                smtpThread = new Thread(() -> {
                    smtpWakeLock.acquire(wakeLockTimeout);
                    synchronized (smtpThreadStartedCond) {
                        smtpThreadStartedCond.notifyAll();
                    }

                    Log.i(TAG, "###################### SMTP-Thread started. ######################");


                    while (true) {
                        if (Thread.interrupted()) {
                            return;
                        }
                        smtpWakeLock.acquire(wakeLockTimeout);
                        performSmtpJobs();
                        smtpWakeLock.release();
                        performSmtpIdle();
                    }
                }, "smtpThread");
                smtpThread.setPriority(Thread.MAX_PRIORITY);
                smtpThread.start();
            }
        }
    }

    private void stopThreads() {
        if (imapThread != null) {
            imapThread.interrupt();
        }
        if (mvboxThread != null) {
            mvboxThread.interrupt();
        }
        if (sentBoxThread != null) {
            sentBoxThread.interrupt();
        }
        if (smtpThread != null) {
            smtpThread.interrupt();
        }
    }

    @Override
    public long handleEvent(final int event, long data1, long data2) {
        switch (event) {
            case DC_EVENT_INFO:
                Log.i(TAG, dataToString(data2));
                break;

            case DC_EVENT_WARNING:
                Log.w(TAG, dataToString(data2));
                break;

            case DC_EVENT_ERROR:
                handleError(event, dataToString(data2));
                break;

            case DC_EVENT_ERROR_NETWORK:
                handleError(event, dataToString(data2));
                break;

            case DC_EVENT_ERROR_SELF_NOT_IN_GROUP:
                handleError(event, dataToString(data2));
                break;

            case DC_EVENT_HTTP_GET:
                // calling this from the main thread may result in NetworkOnMainThreadException error
                String httpContent = null;
                try {
                    URL url = new URL(dataToString(data1));
                    HttpURLConnection urlConnection = (HttpURLConnection) url.openConnection();
                    try {
                        urlConnection.setConnectTimeout(10 * 1000);
                        InputStream inputStream = new BufferedInputStream(urlConnection.getInputStream());

                        BufferedReader r = new BufferedReader(new InputStreamReader(inputStream));

                        StringBuilder total = new StringBuilder();
                        String line;
                        while ((line = r.readLine()) != null) {
                            total.append(line).append('\n');
                        }
                        httpContent = total.toString();
                    } finally {
                        urlConnection.disconnect();
                    }
                } catch (Exception e) {
                    e.printStackTrace();
                }
                return stringToData(httpContent);

            case DC_EVENT_HTTP_POST:
                // calling this from the main thread may result in NetworkOnMainThreadException error
                String postContent = null;
                try {
                    String urlStr = dataToString(data1);
                    String paramStr = "";
                    if (urlStr.contains("?")) {
                        paramStr = urlStr.substring(urlStr.indexOf("?") + 1);
                        urlStr = urlStr.substring(0, urlStr.indexOf("?"));
                    }
                    byte[] bytes = paramStr.getBytes();

                    URL url = new URL(urlStr);
                    HttpURLConnection conn = (HttpURLConnection) url.openConnection();
                    try {
                        conn.setConnectTimeout(15 * 1000);
                        conn.setReadTimeout(15 * 1000);
                        conn.setDoOutput(true);
                        conn.setRequestMethod("POST");
                        conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded");
                        conn.setRequestProperty("Content-Length", String.valueOf(bytes.length));
                        conn.getOutputStream().write(bytes);

                        int responseCode = conn.getResponseCode();
                        BufferedReader br = new BufferedReader(new InputStreamReader(new BufferedInputStream(conn.getInputStream())));
                        StringBuilder total = new StringBuilder();
                        String line;
                        while ((line = br.readLine()) != null) {
                            total.append(line).append('\n');
                        }
                        if (responseCode == HttpURLConnection.HTTP_OK) {
                            postContent = total.toString();
                        } else {
                            Log.i("DeltaChat", String.format("DC_EVENT_HTTP_POST error: %s", total.toString()));
                        }
                    } finally {
                        conn.disconnect();
                    }
                } catch (Exception e) {
                    e.printStackTrace();
                }
                return stringToData(postContent);

            case DC_EVENT_GET_STRING:
                String coreString = null;
                if (coreStrings != null && !coreStrings.isEmpty()) {
                    coreString = coreStrings.get(data1);
                }
                return stringToData(coreString);

            default: {
                final Object data1obj = data1IsString(event) ? dataToString(data1) : data1;
                final Object data2obj = data2IsString(event) ? dataToString(data2) : data2;
                if (eventCenter != null) {
                    eventCenter.sendToObservers(event, data1obj, data2obj);
                }
            }
            break;
        }
        return 0;
    }

    private void handleError(int event, String error) {
        eventCenter.sendToObservers(EVENT_ERROR, event, error);
    }

    String getDbPath() {
        return dbPath;
    }
}
