package cloud.gitwide.ampverse;
import java.net.*;
import java.util.*;
/** Same-device mock HTTP fixtures, NOT second-device LAN acceptance evidence. */
final class CastTestAddress {
    static InetAddress privateAddress()throws SocketException {
        Enumeration<NetworkInterface> networks=NetworkInterface.getNetworkInterfaces();
        while(networks!=null&&networks.hasMoreElements()){
            NetworkInterface network=networks.nextElement();if(!network.isUp())continue;
            Enumeration<InetAddress> addresses=network.getInetAddresses();
            while(addresses.hasMoreElements()){
                InetAddress address=addresses.nextElement();
                if(address instanceof Inet4Address&&address.isSiteLocalAddress()&&!address.isLoopbackAddress())return address;
            }
        }
        org.junit.Assume.assumeTrue("Private IPv4 interface required for this mock fixture",false);
        throw new IllegalStateException("No private IPv4 interface");
    }
}
